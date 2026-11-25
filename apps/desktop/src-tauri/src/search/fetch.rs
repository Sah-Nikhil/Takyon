//! HTTP over WinHTTP (ADR-0019).
//!
//! The OS stack rather than a Rust client: Schannel for TLS, the user's own
//! proxy and CA store for free, and nothing added to a 2.6 MB installer. Every
//! call is blocking and runs on a Turn's thread, never the main one.
//!
//! Two limits are the point. `WinHttpSetTimeouts` bounds one request, and
//! `pages` returns at `DEADLINE` however many are still in flight — one slow
//! page must not hold the whole answer (v0.9 Traps, ADR-0041).
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
const AGENT: &str = "Takyon/0.9 (V3SPER)";

/// Per-request timeouts, milliseconds. Resolve, connect, send, receive.
const TIMEOUT_MS: i32 = 6_000;

/// Per-request timeouts for one `!s` page. Shorter than `TIMEOUT_MS`: a page
/// still connecting at 3 s misses `DEADLINE` anyway, and frees its thread sooner.
const PAGE_TIMEOUT_MS: i32 = 3_000;

/// Total budget for reading pages. Past it the answer is written from what
/// arrived; the rest go in as their snippets. Was 12 s, and waited for all.
pub const DEADLINE: Duration = Duration::from_millis(3_500);

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

/// `get_url`, under `PAGE_TIMEOUT_MS`, reading no body past `until`. The `!s`
/// page read: a host trickling bytes inside each timeout cannot outlive it.
fn get_page(url: &str, until: Instant) -> Result<Response, SearchError> {
    let (host, path, secure) = parse_url(url)
        .ok_or_else(|| SearchError::Failed(format!("{url} is not a URL Takyon can read.")))?;
    let headers = "Accept: text/html\r\n";
    send(
        &host,
        &path,
        headers,
        secure,
        "GET",
        None,
        MAX_BODY,
        PAGE_TIMEOUT_MS,
        Some(until),
    )
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
        if path.is_empty() {
            "/".into()
        } else {
            path.into()
        },
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

/// Threads for blocking network work. Never rayon's global pool: the keystroke
/// fan-out (`query.rs`) shares it, and Focus's sixty first-launch icon fetches
/// held all four of a CI runner's threads until the bench hung (v0.17).
pub fn pool() -> &'static rayon::ThreadPool {
    static POOL: std::sync::OnceLock<rayon::ThreadPool> = std::sync::OnceLock::new();
    POOL.get_or_init(|| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(8)
            .thread_name(|i| format!("takyon-net-{i}"))
            .build()
            .expect("network thread pool")
    })
}

/// Read several pages at once, returning at [`DEADLINE`] or when all are in.
///
/// Failures are per URL and kept: a page that would not load is named in the
/// answer rather than dropped (v0.9 Traps). A straggler finishes on its own
/// thread, unread, under its own `PAGE_TIMEOUT_MS`.
pub fn pages(urls: &[String]) -> Vec<Result<String, SearchError>> {
    pages_with(urls, DEADLINE, get_page)
}

/// `pages`, with the deadline and the reader supplied. For tests.
pub fn pages_with(
    urls: &[String],
    deadline: Duration,
    read: fn(&str, Instant) -> Result<Response, SearchError>,
) -> Vec<Result<String, SearchError>> {
    let (tx, rx) = std::sync::mpsc::channel();
    let until = Instant::now() + deadline;
    for (i, url) in urls.iter().enumerate() {
        let Some(slot) = PageSlot::take() else {
            // Earlier searches' stragglers hold every slot. Snippet, not a thread.
            let _ = tx.send((
                i,
                Err(SearchError::Failed("Too many pages in flight.".into())),
            ));
            continue;
        };
        let (tx, url) = (tx.clone(), url.clone());
        // A thread each, not `pool()`: Focus's icon fetches can hold every pool
        // thread for seconds, and a page queued behind them misses `DEADLINE`.
        std::thread::spawn(move || {
            let _slot = slot;
            let body = read(&url, until).map(|r| {
                if r.status == 200 {
                    r.body
                } else {
                    String::new()
                }
            });
            // Past the deadline the receiver is gone, and nobody wants this.
            let _ = tx.send((i, body));
        });
    }
    drop(tx);
    let mut out: Vec<Option<Result<String, SearchError>>> = urls.iter().map(|_| None).collect();
    let mut left = urls.len();
    while left > 0 {
        let wait = until.saturating_duration_since(Instant::now());
        let Ok((i, body)) = rx.recv_timeout(wait) else {
            break;
        };
        out[i] = Some(body);
        left -= 1;
    }
    out.into_iter()
        .map(|body| {
            body.unwrap_or_else(|| Err(SearchError::Failed("Timed out before reading.".into())))
        })
        .collect()
}

/// Most page threads alive at once, across every search. Stragglers from earlier
/// searches count, so a burst of `!s` on slow hosts cannot pile up threads.
pub const MAX_PAGE_WORKERS: usize = 24;

static PAGE_WORKERS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// One of `MAX_PAGE_WORKERS`, given back when its thread ends.
struct PageSlot;

impl PageSlot {
    fn take() -> Option<PageSlot> {
        use std::sync::atomic::Ordering::SeqCst;
        PAGE_WORKERS
            .fetch_update(SeqCst, SeqCst, |n| (n < MAX_PAGE_WORKERS).then_some(n + 1))
            .ok()
            .map(|_| PageSlot)
    }
}

impl Drop for PageSlot {
    fn drop(&mut self) {
        PAGE_WORKERS.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
    }
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
    send(
        host,
        path,
        &joined,
        true,
        "POST",
        Some(body.as_bytes()),
        MAX_BODY,
        TIMEOUT_MS,
        None,
    )
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
        TIMEOUT_MS,
        None,
    )
}

fn plain(host: &str, path: &str) -> Result<Response, SearchError> {
    send(
        host,
        path,
        "Accept: text/html\r\n",
        false,
        "GET",
        None,
        MAX_BODY,
        TIMEOUT_MS,
        None,
    )
}

fn request(host: &str, path: &str, headers: &str) -> Result<Response, SearchError> {
    send(
        host, path, headers, true, "GET", None, MAX_BODY, TIMEOUT_MS, None,
    )
}

/// The WinHTTP call itself.
///
/// Every handle is closed on every path, including the error ones: a leaked
/// session holds a connection and a thread, and nothing reports it.
#[cfg(windows)]
#[allow(clippy::too_many_arguments)]
fn send(
    host: &str,
    path: &str,
    headers: &str,
    secure: bool,
    method: &str,
    body: Option<&[u8]>,
    cap: usize,
    timeout_ms: i32,
    until: Option<Instant>,
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
        WinHttpSetTimeouts(session, timeout_ms, timeout_ms, timeout_ms, timeout_ms)
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

        let bytes = read_bytes(request, cap, until)?;
        Ok(Response {
            status: status as u16,
            body: String::from_utf8_lossy(&bytes).into_owned(),
            bytes,
        })
    }
}

/// No transport off Windows yet, so every request fails by name.
///
/// Refusing here rather than earlier keeps `!s` honest end to end: the Bang
/// still parses, retrieval still runs its own error path, and the Palette shows
/// this sentence instead of an empty answer. TBC-0013 owns the real one.
#[cfg(not(windows))]
#[allow(clippy::too_many_arguments)]
fn send(
    _host: &str,
    _path: &str,
    _headers: &str,
    _secure: bool,
    _method: &str,
    _body: Option<&[u8]>,
    _cap: usize,
    _timeout_ms: i32,
    _until: Option<Instant>,
) -> Result<Response, SearchError> {
    Err(SearchError::Failed(
        "HTTP is only implemented on Windows; macOS needs URLSession (TBC-0013).".into(),
    ))
}

/// Read the body, stopping at `cap`.
#[cfg(windows)]
unsafe fn read_bytes(
    request: *mut core::ffi::c_void,
    cap: usize,
    until: Option<Instant>,
) -> Result<Vec<u8>, SearchError> {
    let mut body: Vec<u8> = Vec::new();
    loop {
        // Each read is bounded by the receive timeout; the whole body only by this.
        if until.is_some_and(|until| Instant::now() >= until) {
            break;
        }
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

    fn fake_read(url: &str, _until: Instant) -> Result<Response, SearchError> {
        let ok = |body: &str| Response {
            status: 200,
            body: body.into(),
            bytes: Vec::new(),
        };
        match url {
            "slow" => {
                std::thread::sleep(Duration::from_millis(1_500));
                Ok(ok("late"))
            }
            "dead" => Err(SearchError::Failed("no such host".into())),
            "missing" => Ok(Response {
                status: 404,
                ..ok("not found page")
            }),
            other => Ok(ok(other)),
        }
    }

    /// One slow page must not hold the answer: `pages` returns at its deadline
    /// with what arrived, in order, and names the straggler as unread.
    #[test]
    fn v0_11_2_pages_return_at_the_deadline_without_the_straggler() {
        let urls: Vec<String> = ["a", "slow", "dead", "b", "missing"]
            .iter()
            .map(|u| u.to_string())
            .collect();
        let started = Instant::now();
        let bodies = pages_with(&urls, Duration::from_millis(400), fake_read);
        assert!(
            started.elapsed() < Duration::from_millis(1_200),
            "waited for the straggler"
        );
        assert_eq!(bodies.len(), 5);
        assert_eq!(bodies[0].as_deref(), Ok("a"));
        let late = bodies[1].as_ref().unwrap_err().message();
        assert!(late.contains("Timed out"), "{late}");
        assert!(bodies[2].is_err());
        assert_eq!(bodies[3].as_deref(), Ok("b"));
        // A non-200 reads as empty, never as its error page's prose.
        assert_eq!(bodies[4].as_deref(), Ok(""));
    }

    /// Every reader gets the same absolute deadline the caller stops at, so a
    /// page trickling bytes stops reading then rather than at its own leisure.
    #[test]
    fn v0_11_2_every_page_read_is_given_the_callers_deadline() {
        fn checked(_: &str, until: Instant) -> Result<Response, SearchError> {
            let left = until.saturating_duration_since(Instant::now());
            assert!(left > Duration::ZERO && left <= Duration::from_secs(5));
            Ok(Response {
                status: 200,
                body: "ok".into(),
                bytes: Vec::new(),
            })
        }
        let urls = vec!["a".to_string(), "b".to_string()];
        let bodies = pages_with(&urls, Duration::from_secs(5), checked);
        assert!(
            bodies.iter().all(|b| b.as_deref() == Ok("ok")),
            "{bodies:?}"
        );
    }

    /// Everything in before the deadline returns at once, not at the deadline.
    #[test]
    fn v0_11_2_pages_return_as_soon_as_every_page_is_in() {
        let urls = vec!["a".to_string(), "b".to_string()];
        let started = Instant::now();
        let bodies = pages_with(&urls, Duration::from_secs(5), fake_read);
        assert!(started.elapsed() < Duration::from_secs(2));
        assert!(bodies.iter().all(Result::is_ok));
    }

    /// Credentials in a URL are a phishing shape. Refused, never stripped and
    /// sent anyway.
    #[test]
    fn v0_9_a_url_carrying_credentials_is_refused() {
        assert_eq!(parse_url("https://user:pw@example.com/x"), None);
    }
}
