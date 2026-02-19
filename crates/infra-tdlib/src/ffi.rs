use std::ffi::{c_char, c_double, c_int};

// TDLib JSON interface — 4 functions, that's all we need.
// Safety: these functions are thread-safe per TDLib documentation.
// td_send and td_receive can be called from different threads.
// td_receive returns a pointer to a static buffer that is valid
// until the next td_receive call from the SAME thread.
#[link(name = "tdjson")]
extern "C" {
    pub fn td_create_client_id() -> c_int;

    pub fn td_send(client_id: c_int, request: *const c_char);

    pub fn td_receive(timeout: c_double) -> *const c_char;

    pub fn td_execute(request: *const c_char) -> *const c_char;
}
