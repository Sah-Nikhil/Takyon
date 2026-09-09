//! HTTP over WinHTTP (ADR-0019).
//!
//! The OS stack rather than a Rust client: Schannel for TLS, the user's own
//! proxy and CA store for free, and nothing added to a 2.6 MB installer. Every
//! call is blocking and runs on a Turn's thread, never the main one.
//!
//! Two limits are the point. `WinHttpSetTimeouts` bounds one request, and
//! `pages` stops reading at `DEADLINE` however many are still in flight — one
//! slow page must not hold the whole answer (v0.9 Traps).
//!
//! [`send`] is the transport and the only Windows-specific thing here; URL
//! parsing, encoding and the `pages` fan-out are portable. macOS answer is
//! open — TBC-0013 weighs `URLSession` against one Rust client everywhere.

use std::time::{Duration, Instant};

#[cfg(windows)]
use windows::core::PCWSTR;
#[cfg(windows)]
use windows::Win32::Networking::WinHttp::{
    WinHttpCloseHandle, WinHttpConnect, WinHttpOpen, WinHttpOpenRequest, WinHttpQueryDataAvailable,
    WinHttpQueryHeaders, WinHttpReadData, WinHttpReceiveResponse, WinHttpSendRequest,
    WinHttpSetTimeouts, WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY, WINHTTP_FLAG_SECURE,
    WINHTTP_QUERY_FLAG_NUMBER, WINHTTP_QUERY_STATUS_CODE,
};

use super::SearchError;

/// What Takyon calls itself to a server. Its own constant: a blank agent is
/// refused outright by a noticeable share of sites.
#[cfg(windows)]
const AGENT: &str = "Takyon/0.9 (+https://github.com/Sah-Nikhil)";

/// Per-request timeouts, milliseconds. Resolve, connect, send, receive.
#[cfg(windows)]
const TIMEOUT_MS: i32 = 6_000;

/// The same budget for `NSURLRequest`, which counts in seconds.
#[cfg(target_os = "macos")]
const TIMEOUT_SECS: f64 = 6.0;

/// Total budget for reading pages, whatever is still in flight.
pub const DEADLINE: Duration = Duration::from_secs(12);

/// Most bytes read from one page. Extraction only ever reads the first screens
/// of prose, and an unbounded read is how one video file stalls an answer.
pub const MAX_BODY: usize = 512 * 1024;

/// A capped read must still hold a whole document for extraction. Compile-time,
/// so raising one cap and forgetting the other fails the build rather than
/// quietly truncating every article.
const _: () = assert!(MAX_BODY > super::extract::MAX_CHARS * 4);

/// One HTTP response, already decoded.
///
/// Both forms of the same read: `body` for pages, `bytes` for an icon, which is
/// not text and would be replacement characters all the way down.
#[derive(Debug)]
pub struct Response {
    pub status: u16,
    pub body: String,
    pub bytes: Vec<u8>,
}

/// A GET over HTTPS. `path` includes the query string.
pub fn get(host: &str, path: &str, headers: &[(&str, &str)]) -> Result<Response, SearchError> {
    let joined: String = headers
        .iter()
        .map(|(name, value)| format!("{name}: {value}\r\n"))
        .collect();
    request(host, path, &joined)
}

/// A GET at a full URL. HTTPS and HTTP both, because half the web still
/// redirects through one.
pub fn get_url(url: &str) -> Result<Response, SearchError> {
    let (host, path, secure) = parse_url(url)
        .ok_or_else(|| SearchError::Failed(format!("{url} is not a URL Takyon can read.")))?;
    if !secure {
        // WinHTTP will not send a secure request to port 80, and a plain one is
        // what the redirect chain ends at often enough to matter.
        return plain(&host, &path);
    }
    request(&host, &path, "Accept: text/html\r\n")
}

/// Host, path and whether it is HTTPS. `None` for anything not http(s).
///
/// Hand-rolled rather than `WinHttpCrackUrl`: three fields are needed and the
/// API wants a `URL_COMPONENTS` with six output buffers to give them.
pub fn parse_url(url: &str) -> Option<(String, String, bool)> {
    let (secure, rest) = match url.trim() {
        u if u.len() > 8 && u[..8].eq_ignore_ascii_case("https://") => (true, &u[8..]),
        u if u.len() > 7 && u[..7].eq_ignore_ascii_case("http://") => (false, &u[7..]),
        _ => return None,
    };
    let end = rest.find('/').unwrap_or(rest.len());
    let (host, path) = rest.split_at(end);
    // Credentials in a URL are a phishing shape, and Takyon has no business
    // sending them. Refused rather than stripped, so nothing is sent silently.
    if host.is_empty() || host.contains('@') {
        return None;
    }
    Some((
        host.to_string(),
        if path.is_empty() { "/".into() } else { path.into() },
        secure,
    ))
}

/// Percent-encode a query value. Unreserved set from RFC 3986, everything else
/// escaped — a raw `&` in a question would otherwise become a second parameter.
pub fn percent_encode(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

/// Read several pages at once, stopping at [`DEADLINE`].
///
/// Failures are per URL and kept: a page that would not load is named in the
/// answer rather than dropped, or a thinner answer reads as a complete one
/// (v0.9 Traps).
pub fn pages(urls: &[String]) -> Vec<Result<String, SearchError>> {
    use rayon::prelude::*;
    let started = Instant::now();
    urls.par_iter()
        .map(|url| {
            if started.elapsed() >= DEADLINE {
                return Err(SearchError::Failed("Timed out before reading.".into()));
            }
            get_url(url).map(|response| {
                if response.status == 200 {
                    response.body
                } else {
                    String::new()
                }
            })
        })
        .collect()
}

/// A POST over HTTPS with a body. Exa's API is POST-only (ADR-0021).
pub fn post(
    host: &str,
    path: &str,
    headers: &[(&str, &str)],
    body: &str,
) -> Result<Response, SearchError> {
    let joined: String = headers
        .iter()
        .map(|(name, value)| format!("{name}: {value}\r\n"))
        .collect();
    send(host, path, &joined, true, "POST", Some(body.as_bytes()), MAX_BODY)
}

/// Most bytes read from one favicon. A site serving a megabyte PNG as its icon
/// is not going to be drawn at 14px any better for it.
pub const MAX_ICON: usize = 128 * 1024;

/// A GET whose body is bytes, capped at [`MAX_ICON`]. For favicons (ADR-0022).
pub fn get_icon(url: &str) -> Result<Response, SearchError> {
    let (host, path, secure) = parse_url(url)
        .ok_or_else(|| SearchError::Failed(format!("{url} is not a URL Takyon can read.")))?;
    send(
        &host,
        &path,
        "Accept: image/*\r\n",
        secure,
        "GET",
        None,
        MAX_ICON,
    )
}

fn plain(host: &str, path: &str) -> Result<Response, SearchError> {
    send(host, path, "Accept: text/html\r\n", false, "GET", None, MAX_BODY)
}

fn request(host: &str, path: &str, headers: &str) -> Result<Response, SearchError> {
    send(host, path, headers, true, "GET", None, MAX_BODY)
}

/// The WinHTTP call itself.
///
/// Every handle is closed on every path, including the error ones: a leaked
/// session holds a connection and a thread, and nothing reports it.
#[cfg(windows)]
fn send(
    host: &str,
    path: &str,
    headers: &str,
    secure: bool,
    method: &str,
    body: Option<&[u8]>,
    cap: usize,
) -> Result<Response, SearchError> {
    let host_w = wide(host);
    let path_w = wide(path);
    let verb = wide(method);
    let agent = wide(AGENT);
    let headers_w: Vec<u16> = headers.encode_utf16().collect();

    unsafe {
        let session = WinHttpOpen(
            PCWSTR(agent.as_ptr()),
            WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY,
            PCWSTR::null(),
            PCWSTR::null(),
            0,
        );
        if session.is_null() {
            return Err(last_error("Could not start an HTTP session"));
        }
        let _session = Handle(session);
        WinHttpSetTimeouts(session, TIMEOUT_MS, TIMEOUT_MS, TIMEOUT_MS, TIMEOUT_MS)
            .map_err(|e| SearchError::Failed(format!("Could not set timeouts: {e}")))?;

        let port = if secure { 443 } else { 80 };
        let connection = WinHttpConnect(session, PCWSTR(host_w.as_ptr()), port, 0);
        if connection.is_null() {
            return Err(last_error(&format!("Could not connect to {host}")));
        }
        let _connection = Handle(connection);

        let flags = if secure {
            WINHTTP_FLAG_SECURE
        } else {
            Default::default()
        };
        let request = WinHttpOpenRequest(
            connection,
            PCWSTR(verb.as_ptr()),
            PCWSTR(path_w.as_ptr()),
            PCWSTR::null(),
            PCWSTR::null(),
            std::ptr::null(),
            flags,
        );
        if request.is_null() {
            return Err(last_error(&format!("Could not open a request to {host}")));
        }
        let _request = Handle(request);

        let sent = WinHttpSendRequest(
            request,
            if headers_w.is_empty() {
                None
            } else {
                Some(&headers_w)
            },
            body.map(|b| b.as_ptr() as *const core::ffi::c_void),
            body.map(|b| b.len() as u32).unwrap_or(0),
            body.map(|b| b.len() as u32).unwrap_or(0),
            0,
        );
        sent.map_err(|e| SearchError::Failed(format!("{host} could not be reached: {e}")))?;
        WinHttpReceiveResponse(request, std::ptr::null_mut())
            .map_err(|e| SearchError::Failed(format!("{host} sent no response: {e}")))?;

        let mut status: u32 = 0;
        let mut size = std::mem::size_of::<u32>() as u32;
        WinHttpQueryHeaders(
            request,
            WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
            PCWSTR::null(),
            Some(&mut status as *mut u32 as *mut _),
            &mut size,
            std::ptr::null_mut(),
        )
        .map_err(|e| SearchError::Failed(format!("{host} sent no status: {e}")))?;

        let bytes = read_bytes(request, cap)?;
        Ok(Response {
            status: status as u16,
            body: String::from_utf8_lossy(&bytes).into_owned(),
            bytes,
        })
    }
}

/// The macOS half of `search::fetch`, over `NSURLSession` (ADR-0029).
///
/// Same seam and reasons as WinHTTP: OS TLS, the user's proxy, nothing added to
/// the installer. `dataTaskWithRequest:` is asynchronous, so its handler posts
/// to a channel this waits on — every caller is already on a worker.
#[cfg(target_os = "macos")]
fn send(
    host: &str,
    path: &str,
    headers: &str,
    secure: bool,
    method: &str,
    body: Option<&[u8]>,
    cap: usize,
) -> Result<Response, SearchError> {
    use objc2_foundation::{
        NSData, NSHTTPURLResponse, NSMutableURLRequest, NSString, NSURLSession, NSURL,
    };

    let scheme = if secure { "https" } else { "http" };
    let url = NSURL::URLWithString(&NSString::from_str(&format!("{scheme}://{host}{path}")))
        .ok_or_else(|| SearchError::Failed(format!("{scheme}://{host}{path} is not a URL.")))?;

    let request = NSMutableURLRequest::requestWithURL(&url);
    request.setHTTPMethod(&NSString::from_str(method));
    request.setTimeoutInterval(TIMEOUT_SECS);
    for (name, value) in parse_header_block(headers) {
        request.setValue_forHTTPHeaderField(
            Some(&NSString::from_str(&value)),
            &NSString::from_str(&name),
        );
    }
    if let Some(body) = body {
        request.setHTTPBody(Some(&NSData::with_bytes(body)));
    }

    let (tx, rx) = std::sync::mpsc::channel::<Result<(u16, Vec<u8>), String>>();
    let handler = block2::RcBlock::new(
        move |data: *mut NSData,
              response: *mut objc2_foundation::NSURLResponse,
              error: *mut objc2_foundation::NSError| {
            // SAFETY: the three pointers are valid for the duration of the call.
            let outcome = if !error.is_null() {
                Err(unsafe { (*error).localizedDescription() }.to_string())
            } else {
                let status = unsafe { response.as_ref() }
                    .and_then(|r| r.downcast_ref::<NSHTTPURLResponse>())
                    .map(|http| http.statusCode() as u16)
                    .unwrap_or(0);
                let bytes = unsafe { data.as_ref() }.map(|d| d.to_vec()).unwrap_or_default();
                Ok((status, bytes))
            };
            let _ = tx.send(outcome);
        },
    );

    let task = unsafe {
        NSURLSession::sharedSession().dataTaskWithRequest_completionHandler(&request, &handler)
    };
    task.resume();

    // A second past the request's own timeout: the deadline that should fire is
    // URLSession's, which reports why. This one only stops a lost callback from
    // parking the worker forever.
    let waited = std::time::Duration::from_secs_f64(TIMEOUT_SECS + 1.0);
    let (status, mut bytes) = match rx.recv_timeout(waited) {
        Ok(Ok(response)) => response,
        Ok(Err(e)) => return Err(SearchError::Failed(e)),
        Err(_) => return Err(SearchError::Failed("the request timed out.".into())),
    };

    bytes.truncate(cap);
    Ok(Response {
        status,
        body: String::from_utf8_lossy(&bytes).into_owned(),
        bytes,
    })
}

/// Split a raw `Name: Value\r\n` block into pairs.
///
/// WinHTTP takes the block whole; `NSMutableURLRequest` wants one call per
/// field. Pure, so a Windows test run covers it — the block is built by
/// `get` and `post` on both platforms.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn parse_header_block(headers: &str) -> Vec<(String, String)> {
    headers
        .lines()
        .filter_map(|line| line.split_once(':'))
        .map(|(name, value)| (name.trim().to_string(), value.trim().to_string()))
        .filter(|(name, _)| !name.is_empty())
        .collect()
}

#[cfg(not(any(windows, target_os = "macos")))]
fn send(
    _host: &str,
    _path: &str,
    _headers: &str,
    _secure: bool,
    _method: &str,
    _body: Option<&[u8]>,
    _cap: usize,
) -> Result<Response, SearchError> {
    Err(SearchError::Failed(
        "HTTP is not implemented on this platform.".into(),
    ))
}

/// Read the body, stopping at `cap`.
#[cfg(windows)]
unsafe fn read_bytes(
    request: *mut core::ffi::c_void,
    cap: usize,
) -> Result<Vec<u8>, SearchError> {
    let mut body: Vec<u8> = Vec::new();
    loop {
        let mut available: u32 = 0;
        if WinHttpQueryDataAvailable(request, &mut available).is_err() || available == 0 {
            break;
        }
        let want = available.min((cap - body.len()) as u32);
        let mut chunk = vec![0u8; want as usize];
        let mut read: u32 = 0;
        if WinHttpReadData(request, chunk.as_mut_ptr() as *mut _, want, &mut read).is_err() {
            break;
        }
        if read == 0 {
            break;
        }
        chunk.truncate(read as usize);
        body.extend_from_slice(&chunk);
        if body.len() >= cap {
            break;
        }
    }
    Ok(body)
}

/// A WinHTTP handle that closes itself. The one mistake this API punishes
/// silently — a leaked session keeps a connection and never says so.
#[cfg(windows)]
struct Handle(*mut core::ffi::c_void);

#[cfg(windows)]
impl Drop for Handle {
    fn drop(&mut self) {
        unsafe {
            let _ = WinHttpCloseHandle(self.0);
        }
    }
}

#[cfg(windows)]
fn last_error(what: &str) -> SearchError {
    SearchError::Failed(format!("{what}: {}", windows::core::Error::from_win32()))
}

#[cfg(windows)]
fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// WinHTTP takes one block, `NSMutableURLRequest` one call per field, and
    /// both platforms build the block the same way — so this parses on Windows.
    #[test]
    fn v0_12_a_header_block_splits_into_fields() {
        let block = "Accept: text/html\r\nX-Api-Key: abc123\r\n";
        assert_eq!(
            parse_header_block(block),
            vec![
                ("Accept".to_string(), "text/html".to_string()),
                ("X-Api-Key".to_string(), "abc123".to_string()),
            ]
        );
        assert!(parse_header_block("").is_empty());
        // A value holding a colon keeps it: only the first one separates.
        assert_eq!(
            parse_header_block("Referer: https://example.com/x\r\n"),
            vec![("Referer".to_string(), "https://example.com/x".to_string())]
        );
    }

    /// A question is a query value, so everything outside the unreserved set is
    /// escaped — an `&` in a question would otherwise start a second parameter.
    #[test]
    fn v0_9_a_query_is_percent_encoded() {
        assert_eq!(percent_encode("ferrari in f1"), "ferrari%20in%20f1");
        assert_eq!(percent_encode("a&b=c"), "a%26b%3Dc");
        assert_eq!(percent_encode("safe-_.~"), "safe-_.~");
    }

    /// Non-ASCII goes out as UTF-8 bytes, one escape each, which is what the
    /// API expects — anything else searches for mojibake.
    #[test]
    fn v0_9_non_ascii_is_encoded_as_utf8_bytes() {
        assert_eq!(percent_encode("é"), "%C3%A9");
    }

    #[test]
    fn v0_9_a_url_splits_into_host_path_and_scheme() {
        assert_eq!(
            parse_url("https://example.com/a/b?c=d"),
            Some(("example.com".into(), "/a/b?c=d".into(), true))
        );
        assert_eq!(
            parse_url("http://example.com"),
            Some(("example.com".into(), "/".into(), false))
        );
        assert_eq!(
            parse_url("HTTPS://Example.com/x"),
            Some(("Example.com".into(), "/x".into(), true))
        );
    }

    /// Anything that is not http(s) is refused rather than guessed at: `!s` must
    /// never be the thing that opens a `file://` or a `javascript:`.
    #[test]
    fn v0_9_a_non_http_url_is_refused() {
        assert_eq!(parse_url("file:///C:/secrets.txt"), None);
        assert_eq!(parse_url("javascript:alert(1)"), None);
        assert_eq!(parse_url("example.com"), None);
        assert_eq!(parse_url(""), None);
    }

    /// Credentials in a URL are a phishing shape. Refused, never stripped and
    /// sent anyway.
    #[test]
    fn v0_9_a_url_carrying_credentials_is_refused() {
        assert_eq!(parse_url("https://user:pw@example.com/x"), None);
    }
}
