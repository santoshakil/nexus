use std::collections::HashMap;
use std::ffi::{CStr, CString};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use nexus_error::AgentError;
use serde_json::Value;
use tokio::sync::{mpsc, oneshot};
use tracing::{debug, trace, warn};

use crate::ffi;

type PendingMap = Arc<Mutex<HashMap<String, oneshot::Sender<Value>>>>;

pub struct TdClient {
    client_id: i32,
    pending: PendingMap,
    #[allow(dead_code)]
    auth_tx: mpsc::UnboundedSender<Value>,
    auth_rx: Mutex<Option<mpsc::UnboundedReceiver<Value>>>,
    next_id: AtomicU64,
    running: Arc<AtomicBool>,
}

impl Default for TdClient {
    fn default() -> Self {
        Self::new()
    }
}

impl TdClient {
    pub fn new() -> Self {
        // Safety: td_execute is thread-safe. Set TDLib's internal C++ logging
        // to verbosity 1 (errors only) so it doesn't flood stderr with debug spam.
        // Must happen before td_create_client_id or TDLib logs at level 3 by default.
        if let Ok(req) = CString::new(
            r#"{"@type":"setLogVerbosityLevel","new_verbosity_level":1}"#,
        ) {
            unsafe { ffi::td_execute(req.as_ptr()) };
        }

        // Safety: td_create_client_id is thread-safe, returns a new client ID
        let client_id = unsafe { ffi::td_create_client_id() };
        debug!(client_id, "created TDLib client");

        let pending: PendingMap = Arc::new(Mutex::new(HashMap::new()));
        let running = Arc::new(AtomicBool::new(true));
        let (auth_tx, auth_rx) = mpsc::unbounded_channel();

        let pending_clone = pending.clone();
        let running_clone = running.clone();
        let auth_tx_clone = auth_tx.clone();

        // Dedicated OS thread for td_receive (blocking call)
        // Safety: td_receive is thread-safe. We run it on a dedicated thread
        // to avoid blocking tokio worker threads. The thread lives as long as
        // `running` is true. On Drop, we set running=false and the thread exits
        // within 1 second (the td_receive timeout).
        std::thread::Builder::new()
            .name("tdlib-recv".into())
            .spawn(move || {
                receive_loop(pending_clone, auth_tx_clone, running_clone);
            })
            .ok();

        // Kick TDLib's internal processing — managed clients don't start
        // sending updateAuthorizationState until they receive a request.
        // Safety: td_send is thread-safe, client_id is valid, CString is null-terminated.
        if let Ok(init_req) = CString::new(
            r#"{"@type":"getOption","name":"version","@extra":"_init"}"#,
        ) {
            unsafe { ffi::td_send(client_id, init_req.as_ptr()) };
        }

        Self {
            client_id,
            pending,
            auth_tx,
            auth_rx: Mutex::new(Some(auth_rx)),
            next_id: AtomicU64::new(1),
            running,
        }
    }

    pub fn take_auth_rx(&self) -> Option<mpsc::UnboundedReceiver<Value>> {
        match self.auth_rx.lock() {
            Ok(mut guard) => guard.take(),
            Err(_) => None,
        }
    }

    pub async fn send(&self, mut request: Value) -> Result<Value, AgentError> {
        let extra = format!("r{}", self.next_id.fetch_add(1, Ordering::Relaxed));
        request["@extra"] = Value::String(extra.clone());

        let (tx, rx) = oneshot::channel();

        {
            let mut map = self
                .pending
                .lock()
                .map_err(|e| AgentError::internal(format!("lock poisoned: {e}")))?;
            map.insert(extra.clone(), tx);
        }

        let json = serde_json::to_string(&request)
            .map_err(|e| AgentError::internal(e.to_string()))?;

        let c_str =
            CString::new(json).map_err(|e| AgentError::internal(e.to_string()))?;

        // Safety: td_send is thread-safe. client_id is valid (created by
        // td_create_client_id). c_str is a valid null-terminated C string.
        // td_send copies the string internally, so c_str can be dropped after.
        unsafe {
            ffi::td_send(self.client_id, c_str.as_ptr());
        }

        let response = tokio::time::timeout(Duration::from_secs(30), rx)
            .await
            .map_err(|_| {
                self.cleanup_pending(&extra);
                AgentError::network("request timed out after 30s")
            })?
            .map_err(|_| AgentError::internal("response channel dropped"))?;

        check_tdlib_error(&response)?;
        Ok(response)
    }

    pub fn execute_sync(request: &Value) -> Result<Value, AgentError> {
        let json = serde_json::to_string(request)
            .map_err(|e| AgentError::internal(e.to_string()))?;
        let c_str =
            CString::new(json).map_err(|e| AgentError::internal(e.to_string()))?;

        // Safety: td_execute is thread-safe. c_str is valid null-terminated.
        // The returned pointer is valid until the next td_execute call on this thread.
        // We immediately copy the data into a Rust String before any other call.
        let ptr = unsafe { ffi::td_execute(c_str.as_ptr()) };
        if ptr.is_null() {
            return Err(AgentError::internal("td_execute returned null"));
        }

        // Safety: ptr is non-null, points to valid UTF-8 (TDLib guarantees JSON output)
        let c_str = unsafe { CStr::from_ptr(ptr) };
        let json_str = c_str
            .to_str()
            .map_err(|e| AgentError::internal(format!("invalid UTF-8: {e}")))?;

        let val: Value = serde_json::from_str(json_str)
            .map_err(|e| AgentError::internal(format!("json parse: {e}")))?;

        check_tdlib_error(&val)?;
        Ok(val)
    }

    pub fn client_id(&self) -> i32 {
        self.client_id
    }

    fn cleanup_pending(&self, extra: &str) {
        if let Ok(mut map) = self.pending.lock() {
            map.remove(extra);
        }
    }
}

impl Drop for TdClient {
    fn drop(&mut self) {
        self.running.store(false, Ordering::Relaxed);
        debug!(client_id = self.client_id, "TDLib client shutting down");
    }
}

fn receive_loop(
    pending: PendingMap,
    auth_tx: mpsc::UnboundedSender<Value>,
    running: Arc<AtomicBool>,
) {
    debug!("TDLib receive loop started");

    while running.load(Ordering::Relaxed) {
        // Safety: td_receive is thread-safe. timeout=1.0 means block for
        // up to 1 second. Returns null if no data available. The returned
        // pointer is valid until the next td_receive call on THIS thread.
        let ptr = unsafe { ffi::td_receive(1.0) };
        if ptr.is_null() {
            continue;
        }

        // Safety: ptr is non-null, from td_receive. We copy immediately
        // before the next td_receive call invalidates it.
        let c_str = unsafe { CStr::from_ptr(ptr) };
        let json_str = match c_str.to_str() {
            Ok(s) => s,
            Err(e) => {
                warn!("invalid UTF-8 from TDLib: {e}");
                continue;
            }
        };

        let value: Value = match serde_json::from_str(json_str) {
            Ok(v) => v,
            Err(e) => {
                warn!("invalid JSON from TDLib: {e}");
                continue;
            }
        };

        let type_str = value
            .get("@type")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        // Route responses (have @extra) to pending request channels
        if let Some(extra) = value.get("@extra").and_then(|v| v.as_str()) {
            let tx = pending.lock().ok().and_then(|mut p| p.remove(extra));
            if let Some(tx) = tx {
                let _ = tx.send(value);
            } else {
                trace!(extra, "received response for unknown request");
            }
            continue;
        }

        // Route auth state updates to auth channel
        if type_str == "updateAuthorizationState" {
            let _ = auth_tx.send(value);
            continue;
        }

        // Log other updates
        trace!(update_type = type_str, "received update");
    }

    debug!("TDLib receive loop stopped");
}

fn check_tdlib_error(val: &Value) -> Result<(), AgentError> {
    if val.get("@type").and_then(|v| v.as_str()) == Some("error") {
        let code = val.get("code").and_then(|v| v.as_i64()).unwrap_or(0);
        let msg = val
            .get("message")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown error");
        return Err(AgentError::api(format!("TDLib error {code}: {msg}")));
    }
    Ok(())
}
