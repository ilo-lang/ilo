//! WASM HTTP backend — routes `get` and `pst` through a `fetch` host import.
//!
//! ## Architecture
//!
//! Three concrete implementations exist for the `HttpBackend` trait:
//!
//! * [`NativeHttpBackend`] — uses `minreq` (the existing path, native builds).
//! * [`WasmFetchBackend`] — calls into a host-provided `fetch` import
//!   (`wasm32-wasi` / `wasm32-unknown-unknown`).
//! * [`StubHttpBackend`] — returns `Err("http not available")` immediately;
//!   used when neither `http` feature nor WASM fetch is compiled in.
//!
//! The interpreter selects the right backend via `cfg` at compile time;
//! the `HttpBackend` trait normalises the call-site so the dispatch arms stay
//! identical.
//!
//! ## WASM host import contract (`wasm32` targets)
//!
//! The host must export two functions under the module name `ilo_http`:
//!
//! ```text
//! (func $ilo_http_get
//!   (import "ilo_http" "get")
//!   (param $url_ptr i32) (param $url_len i32)
//!   (param $hdr_ptr i32) (param $hdr_len i32)
//!   (result i32))   ; returns a handle
//!
//! (func $ilo_http_post
//!   (import "ilo_http" "post")
//!   (param $url_ptr i32) (param $url_len i32)
//!   (param $body_ptr i32) (param $body_len i32)
//!   (param $hdr_ptr i32) (param $hdr_len i32)
//!   (result i32))   ; returns a handle
//!
//! (func $ilo_http_response_status
//!   (import "ilo_http" "response_status")
//!   (param $handle i32)
//!   (result i32))
//!
//! (func $ilo_http_response_body_len
//!   (import "ilo_http" "response_body_len")
//!   (param $handle i32)
//!   (result i32))
//!
//! (func $ilo_http_response_body_read
//!   (import "ilo_http" "response_body_read")
//!   (param $handle i32) (param $buf_ptr i32) (param $buf_len i32)
//!   (result i32))   ; bytes written
//!
//! (func $ilo_http_response_free
//!   (import "ilo_http" "response_free")
//!   (param $handle i32))
//! ```
//!
//! **Headers format** (`hdr_ptr`/`hdr_len`): a UTF-8 string of
//! `"key1\x00value1\x00key2\x00value2\x00"` — null-byte delimited key/value
//! pairs, terminated by an extra null byte.  An empty header block is a
//! zero-length string (pass a valid pointer with `hdr_len = 0`).
//!
//! A handle of `0` signals transport failure; the body of handle `0` is
//! an ASCII error message.  Status `0` also means transport failure (not
//! an HTTP status code).
//!
//! ## WASI Preview 2 transition path
//!
//! When the `wasi:http/outgoing-handler` interface stabilises in the Rust
//! toolchain, this module will be replaced with a WIT-bound implementation.
//! The `HttpBackend` trait ensures the interpreter dispatch arms need zero
//! changes at that point — only this file and `Cargo.toml` change.

use std::sync::Arc;

use super::{MapKey, Value};

// ── Trait ─────────────────────────────────────────────────────────────────────

/// Synchronous HTTP backend abstraction.
///
/// Both `get` and `post` are synchronous from the caller's perspective even on
/// WASM; the WASM implementation drives the host's async `fetch` via a
/// blocking adapter (spin-loop on the handle until the host resolves it, or a
/// host-provided blocking call — host implementors should use the latter).
pub trait HttpBackend {
    /// Perform an HTTP GET.  Returns `Ok(body_text)` or `Err(message)`.
    fn get(&self, url: &str, headers: &[(String, String)]) -> Result<String, String>;

    /// Perform an HTTP POST with a text body.  Returns `Ok(body_text)` or
    /// `Err(message)`.
    fn post(&self, url: &str, body: &str, headers: &[(String, String)]) -> Result<String, String>;

    /// Stream an HTTP GET response as lines (ILO-46 client side). Returns a
    /// boxed `Iterator<Item = Result<String, String>>` that drains the body
    /// one line at a time as bytes arrive — never buffers the full response.
    /// Each `Ok(line)` is one chunk-line (newline stripped, trailing `\r`
    /// trimmed for `\r\n` chunked encoding). Transport / I/O errors surface
    /// as `Err(msg)` from the iterator; the outer `Result` only fails when
    /// the initial connection can't be opened.
    fn get_stream(
        &self,
        url: &str,
        headers: &[(String, String)],
    ) -> Result<Box<dyn Iterator<Item = std::result::Result<String, std::io::Error>> + Send>, String>;

    /// Stream an HTTP POST response as lines. Same semantics as `get_stream`.
    fn post_stream(
        &self,
        url: &str,
        body: &str,
        headers: &[(String, String)],
    ) -> Result<Box<dyn Iterator<Item = std::result::Result<String, std::io::Error>> + Send>, String>;
}

// ── Native backend (minreq) ───────────────────────────────────────────────────

/// Native HTTP backend that uses `minreq`.  Present on non-WASM builds when
/// the `http` feature is enabled.
#[cfg(all(feature = "http", not(target_arch = "wasm32")))]
pub struct NativeHttpBackend;

#[cfg(all(feature = "http", not(target_arch = "wasm32")))]
impl HttpBackend for NativeHttpBackend {
    fn get(&self, url: &str, headers: &[(String, String)]) -> Result<String, String> {
        let mut req = minreq::get(url);
        for (k, v) in headers {
            req = req.with_header(k.as_str(), v.as_str());
        }
        req.send()
            .map_err(|e| e.to_string())
            .and_then(|r| r.as_str().map(|s| s.to_owned()).map_err(|e| e.to_string()))
    }

    fn post(&self, url: &str, body: &str, headers: &[(String, String)]) -> Result<String, String> {
        let mut req = minreq::post(url).with_body(body);
        for (k, v) in headers {
            req = req.with_header(k.as_str(), v.as_str());
        }
        req.send()
            .map_err(|e| e.to_string())
            .and_then(|r| r.as_str().map(|s| s.to_owned()).map_err(|e| e.to_string()))
    }

    fn get_stream(
        &self,
        url: &str,
        headers: &[(String, String)],
    ) -> Result<Box<dyn Iterator<Item = std::result::Result<String, std::io::Error>> + Send>, String>
    {
        let mut req = minreq::get(url);
        for (k, v) in headers {
            req = req.with_header(k.as_str(), v.as_str());
        }
        let resp = req.send_lazy().map_err(|e| e.to_string())?;
        Ok(Box::new(LazyLineSplitter::new(resp)))
    }

    fn post_stream(
        &self,
        url: &str,
        body: &str,
        headers: &[(String, String)],
    ) -> Result<Box<dyn Iterator<Item = std::result::Result<String, std::io::Error>> + Send>, String>
    {
        let mut req = minreq::post(url).with_body(body);
        for (k, v) in headers {
            req = req.with_header(k.as_str(), v.as_str());
        }
        let resp = req.send_lazy().map_err(|e| e.to_string())?;
        Ok(Box::new(LazyLineSplitter::new(resp)))
    }
}

/// Splits a `minreq::ResponseLazy` byte stream into lines, yielding each line
/// the instant a `\n` is seen rather than waiting for a read buffer to fill.
///
/// `ResponseLazy` is an `Iterator<Item = io::Result<(u8, usize)>>` that decodes
/// the (possibly chunked) body one byte at a time. Consuming it directly, and
/// emitting a `String` the moment a newline arrives, gives event-bound latency
/// for SSE-style upstreams that flush a short line then idle, instead of the
/// buffer-bound latency a `BufReader::lines()` wrapper imposes (it blocks on a
/// full ~8 KiB fill before surfacing any line). See ILO-489.
///
/// `\n` terminates a line; a trailing `\r` (CRLF / chunked encoding) is
/// stripped to match the previous `BufReader::lines()` behaviour. At EOF any
/// buffered partial line (no final newline) is yielded once, then iteration
/// ends. Read errors surface as `Err(io::Error)`, consistent with the
/// `ILO-R009 http-stream read error` path the interpreter already raises.
#[cfg(all(feature = "http", not(target_arch = "wasm32")))]
struct LazyLineSplitter {
    bytes: minreq::ResponseLazy,
    buf: Vec<u8>,
    done: bool,
}

#[cfg(all(feature = "http", not(target_arch = "wasm32")))]
impl LazyLineSplitter {
    fn new(bytes: minreq::ResponseLazy) -> Self {
        LazyLineSplitter {
            bytes,
            buf: Vec::new(),
            done: false,
        }
    }

    /// Turn the accumulated `buf` into a `String`, stripping a trailing `\r`,
    /// and reset the buffer for the next line.
    fn take_line(&mut self) -> std::result::Result<String, std::io::Error> {
        if self.buf.last() == Some(&b'\r') {
            self.buf.pop();
        }
        let line = String::from_utf8_lossy(&self.buf).into_owned();
        self.buf.clear();
        Ok(line)
    }
}

#[cfg(all(feature = "http", not(target_arch = "wasm32")))]
impl Iterator for LazyLineSplitter {
    type Item = std::result::Result<String, std::io::Error>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.done {
            return None;
        }
        loop {
            match self.bytes.next() {
                Some(Ok((b'\n', _))) => return Some(self.take_line()),
                Some(Ok((byte, _))) => self.buf.push(byte),
                Some(Err(e)) => {
                    self.done = true;
                    let io_err = match e {
                        minreq::Error::IoError(e) => e,
                        other => std::io::Error::other(other.to_string()),
                    };
                    return Some(Err(io_err));
                }
                None => {
                    // EOF: emit any buffered trailing partial line once.
                    self.done = true;
                    if self.buf.is_empty() {
                        return None;
                    }
                    return Some(self.take_line());
                }
            }
        }
    }
}

// ── WASM fetch backend ────────────────────────────────────────────────────────

/// WASM HTTP backend that routes requests through a host-provided `fetch`
/// import.  See module-level docs for the host import contract.
#[cfg(target_arch = "wasm32")]
pub struct WasmFetchBackend;

// Raw host imports.  The host (browser, Deno, Cloudflare Workers, a custom
// wasmtime host) must provide these under module `"ilo_http"`.
#[cfg(target_arch = "wasm32")]
#[link(wasm_import_module = "ilo_http")]
unsafe extern "C" {
    /// Issue an HTTP GET.  Returns a response handle (0 = transport error).
    fn ilo_http_get(url_ptr: *const u8, url_len: usize, hdr_ptr: *const u8, hdr_len: usize) -> u32;

    /// Issue an HTTP POST.  Returns a response handle (0 = transport error).
    fn ilo_http_post(
        url_ptr: *const u8,
        url_len: usize,
        body_ptr: *const u8,
        body_len: usize,
        hdr_ptr: *const u8,
        hdr_len: usize,
    ) -> u32;

    /// HTTP status code for `handle`.  Returns 0 on transport error.
    fn ilo_http_response_status(handle: u32) -> u32;

    /// Byte length of the response body for `handle`.
    fn ilo_http_response_body_len(handle: u32) -> u32;

    /// Read up to `buf_len` bytes of the response body into `buf_ptr`.
    /// Returns the number of bytes written.
    fn ilo_http_response_body_read(handle: u32, buf_ptr: *mut u8, buf_len: usize) -> u32;

    /// Release host-side resources for `handle`.
    fn ilo_http_response_free(handle: u32);
}

/// Encode a `&[(String, String)]` header slice into the null-delimited wire
/// format expected by the host import.
#[cfg(target_arch = "wasm32")]
fn encode_headers(headers: &[(String, String)]) -> Vec<u8> {
    let mut buf = Vec::new();
    for (k, v) in headers {
        buf.extend_from_slice(k.as_bytes());
        buf.push(0);
        buf.extend_from_slice(v.as_bytes());
        buf.push(0);
    }
    buf
}

#[cfg(target_arch = "wasm32")]
impl HttpBackend for WasmFetchBackend {
    fn get(&self, url: &str, headers: &[(String, String)]) -> Result<String, String> {
        let hdr_buf = encode_headers(headers);
        let handle =
            unsafe { ilo_http_get(url.as_ptr(), url.len(), hdr_buf.as_ptr(), hdr_buf.len()) };
        read_response(handle)
    }

    fn post(&self, url: &str, body: &str, headers: &[(String, String)]) -> Result<String, String> {
        let hdr_buf = encode_headers(headers);
        let handle = unsafe {
            ilo_http_post(
                url.as_ptr(),
                url.len(),
                body.as_ptr(),
                body.len(),
                hdr_buf.as_ptr(),
                hdr_buf.len(),
            )
        };
        read_response(handle)
    }

    fn get_stream(
        &self,
        _url: &str,
        _headers: &[(String, String)],
    ) -> Result<Box<dyn Iterator<Item = std::result::Result<String, std::io::Error>> + Send>, String>
    {
        Err("HTTP streaming not supported on this build".to_string())
    }

    fn post_stream(
        &self,
        _url: &str,
        _body: &str,
        _headers: &[(String, String)],
    ) -> Result<Box<dyn Iterator<Item = std::result::Result<String, std::io::Error>> + Send>, String>
    {
        Err("HTTP streaming not supported on this build".to_string())
    }
}

/// Read the body from a response handle, then free it.
///
/// A handle of `0` signals transport failure; the body bytes of handle `0`
/// contain an error message from the host.
#[cfg(target_arch = "wasm32")]
fn read_response(handle: u32) -> Result<String, String> {
    let body_len = unsafe { ilo_http_response_body_len(handle) } as usize;
    let mut buf = vec![0u8; body_len];
    if body_len > 0 {
        unsafe { ilo_http_response_body_read(handle, buf.as_mut_ptr(), buf.len()) };
    }
    let status = unsafe { ilo_http_response_status(handle) };
    unsafe { ilo_http_response_free(handle) };

    let text = String::from_utf8(buf).map_err(|e| format!("response is not valid UTF-8: {e}"))?;

    if handle == 0 || status == 0 {
        Err(text)
    } else {
        Ok(text)
    }
}

// ── Stub backend ──────────────────────────────────────────────────────────────

/// Fallback backend returned when neither `http` nor WASM fetch is available.
pub struct StubHttpBackend;

impl HttpBackend for StubHttpBackend {
    fn get(&self, _url: &str, _headers: &[(String, String)]) -> Result<String, String> {
        Err("http feature not enabled".to_string())
    }

    fn post(
        &self,
        _url: &str,
        _body: &str,
        _headers: &[(String, String)],
    ) -> Result<String, String> {
        Err("http feature not enabled".to_string())
    }

    fn get_stream(
        &self,
        _url: &str,
        _headers: &[(String, String)],
    ) -> Result<Box<dyn Iterator<Item = std::result::Result<String, std::io::Error>> + Send>, String>
    {
        Err("HTTP streaming not supported on this build".to_string())
    }

    fn post_stream(
        &self,
        _url: &str,
        _body: &str,
        _headers: &[(String, String)],
    ) -> Result<Box<dyn Iterator<Item = std::result::Result<String, std::io::Error>> + Send>, String>
    {
        Err("HTTP streaming not supported on this build".to_string())
    }
}

// ── Factory ───────────────────────────────────────────────────────────────────

/// Return a boxed backend appropriate for the current compile target and
/// feature flags.
///
/// Selection order:
/// 1. WASM target → `WasmFetchBackend` (regardless of `http` feature).
/// 2. Non-WASM + `http` feature → `NativeHttpBackend`.
/// 3. Otherwise → `StubHttpBackend`.
pub fn default_backend() -> Box<dyn HttpBackend> {
    #[cfg(target_arch = "wasm32")]
    {
        Box::new(WasmFetchBackend)
    }
    #[cfg(all(feature = "http", not(target_arch = "wasm32")))]
    {
        Box::new(NativeHttpBackend)
    }
    #[cfg(all(not(feature = "http"), not(target_arch = "wasm32")))]
    {
        Box::new(StubHttpBackend)
    }
}

// ── Value helpers ─────────────────────────────────────────────────────────────

/// Convert a backend `Result<String, String>` to an ilo `R t t` value.
pub fn result_to_value(r: Result<String, String>) -> Value {
    match r {
        Ok(body) => Value::Ok(Box::new(Value::Text(Arc::new(body)))),
        Err(msg) => Value::Err(Box::new(Value::Text(Arc::new(msg)))),
    }
}

/// Extract headers from an ilo `M t t` value into a `Vec<(String, String)>`.
pub fn map_to_headers(map: &std::collections::HashMap<MapKey, Value>) -> Vec<(String, String)> {
    map.iter()
        .map(|(k, v)| {
            let key = k.to_display_string();
            let val = match v {
                Value::Text(s) => (**s).clone(),
                other => format!("{other:?}"),
            };
            (key, val)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stub_get_returns_err() {
        let b = StubHttpBackend;
        let r = b.get("https://example.com", &[]);
        assert!(r.is_err());
        assert!(r.unwrap_err().contains("http feature not enabled"));
    }

    #[test]
    fn stub_post_returns_err() {
        let b = StubHttpBackend;
        let r = b.post("https://example.com", "body", &[]);
        assert!(r.is_err());
    }

    #[test]
    fn result_to_value_ok() {
        let v = result_to_value(Ok("hello".to_string()));
        assert!(matches!(v, Value::Ok(_)));
    }

    #[test]
    fn result_to_value_err() {
        let v = result_to_value(Err("boom".to_string()));
        assert!(matches!(v, Value::Err(_)));
    }

    #[cfg(target_arch = "wasm32")]
    #[test]
    fn encode_headers_empty() {
        assert!(encode_headers(&[]).is_empty());
    }

    #[cfg(target_arch = "wasm32")]
    #[test]
    fn encode_headers_one_pair() {
        let enc = encode_headers(&[("Content-Type".to_string(), "application/json".to_string())]);
        // key \0 value \0
        let expected = b"Content-Type\x00application/json\x00";
        assert_eq!(&enc, expected);
    }
}
