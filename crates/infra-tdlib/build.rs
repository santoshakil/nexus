fn main() {
    println!("cargo:rustc-link-lib=tdjson");
    if let Ok(dir) = std::env::var("TDLIB_DIR") {
        println!("cargo:rustc-link-search=native={dir}/lib");
    }
    if let Ok(dir) = std::env::var("TDLIB_INCLUDE_DIR") {
        println!("cargo:include={dir}");
    }
}
