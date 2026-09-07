//! Windows Search, for locations outside the walked roots (§5 task 9).
//!
//! **Default off, and never on the fast path.** Measured on the development
//! machine: 10, 12, 26 and 72 ms for trivial filters against a 20 ms p95 budget.
//! It is asked only after the local index has answered, and only when that answer
//! came up short.
//!
//! Its coverage is also not something to rely on. The same machine has the whole
//! C: drive in crawl scope and returns **zero rows** for `C:\Programming\SELF`,
//! because per-folder exclusions are invisible until a query comes back empty.
//! TBC-0005's amendment carries the measurement. That is why this is a fallback
//! and not the mechanism.

#[cfg(windows)]
use std::path::PathBuf;

use crate::index::FileHit;
#[cfg(windows)]
use crate::rank;

/// The OLE DB provider that answers `SystemIndex` queries.
#[cfg(windows)]
const PROVIDER: &str = "Provider=Search.CollatorDSO;Extended Properties='Application=Windows'";

/// A query against the OS index. No state: the service holds it all.
pub struct WindowsSearch;

impl WindowsSearch {
    /// Filenames matching `needle`, or nothing if the service will not answer.
    ///
    /// Nothing is the normal failure, not an error: `WSearch` is disabled by
    /// users, optimisers and group policy, and a fallback that reports its own
    /// absence has nothing useful to say to someone who never turned it on.
    #[cfg(windows)]
    pub fn search(needle: &str, limit: usize) -> Vec<FileHit> {
        if needle.trim().is_empty() || limit == 0 {
            return Vec::new();
        }
        let sql = format!(
            "SELECT TOP {limit} System.ItemPathDisplay, System.ItemType \
             FROM SystemIndex WHERE System.FileName LIKE '{}%'",
            escape(needle)
        );
        match run(&sql) {
            Ok(paths) => paths.into_iter().filter_map(hit_of).collect(),
            Err(_) => Vec::new(),
        }
    }

    #[cfg(not(windows))]
    pub fn search(_needle: &str, _limit: usize) -> Vec<FileHit> {
        Vec::new()
    }
}

/// Single quotes end a literal, so a name containing one would end the query.
///
/// Doubling is what the OLE DB dialect wants. Every other character is safe:
/// the needle only ever reaches a `LIKE` on a filename.
#[cfg(windows)]
fn escape(needle: &str) -> String {
    needle.replace('\'', "''")
}

/// One hit, scored a rung below anything the local index could have produced.
///
/// A fallback answer is from outside the roots the user chose, so it is a wider
/// guess than a local one and must not outrank it.
#[cfg(windows)]
fn hit_of(path: String) -> Option<FileHit> {
    let path = PathBuf::from(path);
    let is_dir = path.is_dir();
    Some(FileHit {
        score: rank::TIER_EXE_PREFIX,
        is_dir,
        path,
    })
}

/// Run one query through the `Search.CollatorDSO` provider.
///
/// COM `IDispatch` on ADODB rather than a typed OLE DB binding: the provider has
/// no Rust binding, this runs at most once per keystroke and only when the
/// fallback is on, and `IDispatch` is dozens of lines against hundreds.
#[cfg(windows)]
fn run(sql: &str) -> Result<Vec<String>, windows::core::Error> {
    use windows::core::BSTR;
    use windows::Win32::System::Variant::VARIANT;
    use windows::Win32::System::Com::{
        CoCreateInstance, CLSCTX_INPROC_SERVER, DISPATCH_METHOD, DISPATCH_PROPERTYGET, DISPPARAMS,
        IDispatch,
    };
    use windows::Win32::System::Variant::VT_BSTR;

    let _com = crate::com::ComScope::new();

    // SAFETY: a documented, registered ProgID; the interface is released when the
    // binding drops.
    let connection: IDispatch = unsafe {
        let clsid = windows::Win32::System::Com::CLSIDFromProgID(windows::core::w!("ADODB.Connection"))?;
        CoCreateInstance(&clsid, None, CLSCTX_INPROC_SERVER)?
    };

    invoke(&connection, "Open", &[VARIANT::from(PROVIDER)], DISPATCH_METHOD)?;
    let recordset = invoke(&connection, "Execute", &[VARIANT::from(sql)], DISPATCH_METHOD)?;
    let recordset: IDispatch = (&recordset).try_into()?;

    let mut out = Vec::new();
    loop {
        let eof = invoke(&recordset, "EOF", &[], DISPATCH_PROPERTYGET)?;
        if bool::try_from(&eof).unwrap_or(true) {
            break;
        }
        let fields: IDispatch = (&invoke(&recordset, "Fields", &[], DISPATCH_PROPERTYGET)?).try_into()?;
        let item = invoke(&fields, "Item", &[VARIANT::from(0i32)], DISPATCH_PROPERTYGET)?;
        let item: IDispatch = (&item).try_into()?;
        let value = invoke(&item, "Value", &[], DISPATCH_PROPERTYGET)?;
        // A row whose path is not a string is a row we cannot open. Skipped
        // rather than failing the whole query.
        if unsafe { value.Anonymous.Anonymous.vt } == VT_BSTR {
            out.push(BSTR::try_from(&value).unwrap_or_default().to_string());
        }
        invoke(&recordset, "MoveNext", &[], DISPATCH_METHOD)?;
    }

    let _ = invoke(&connection, "Close", &[], DISPATCH_METHOD);
    let _ = DISPPARAMS::default();
    Ok(out)
}

/// One `IDispatch` call by name.
///
/// Arguments go in reverse, which is the calling convention and the single most
/// common way to get `IDispatch` subtly wrong.
#[cfg(windows)]
fn invoke(
    object: &windows::Win32::System::Com::IDispatch,
    name: &str,
    args: &[windows::Win32::System::Variant::VARIANT],
    flags: windows::Win32::System::Com::DISPATCH_FLAGS,
) -> Result<windows::Win32::System::Variant::VARIANT, windows::core::Error> {
    use windows::core::{HSTRING, PCWSTR};
    use windows::Win32::System::Variant::VARIANT;
    use windows::Win32::System::Com::DISPPARAMS;

    let wide = HSTRING::from(name);
    let mut names = [PCWSTR(wide.as_ptr())];
    let mut id = 0i32;
    // SAFETY: `names` lives for the call and `id` receives one DISPID.
    unsafe {
        object.GetIDsOfNames(
            &windows::core::GUID::zeroed(),
            names.as_mut_ptr(),
            1,
            0,
            &mut id,
        )?;
    }

    let mut reversed: Vec<VARIANT> = args.iter().rev().cloned().collect();
    let params = DISPPARAMS {
        rgvarg: reversed.as_mut_ptr(),
        cArgs: reversed.len() as u32,
        ..Default::default()
    };
    let mut result = VARIANT::default();
    // SAFETY: `params` borrows `reversed`, which outlives the call.
    unsafe {
        object.Invoke(
            id,
            &windows::core::GUID::zeroed(),
            0,
            flags,
            &params,
            Some(&mut result),
            None,
            None,
        )?;
    }
    Ok(result)
}

// Windows-only: every helper under test speaks OLE DB to Windows Search.
#[cfg(all(test, windows))]
mod tests {
    use super::*;

    /// A name with an apostrophe must not end the SQL literal early.
    #[test]
    fn v0_7_a_quote_in_a_needle_is_escaped() {
        assert_eq!(escape("o'brien"), "o''brien");
        assert_eq!(escape("plain"), "plain");
    }

    /// An empty needle would ask the OS for everything it has indexed.
    #[test]
    fn v0_7_the_fallback_declines_an_empty_needle() {
        assert!(WindowsSearch::search("", 10).is_empty());
        assert!(WindowsSearch::search("   ", 10).is_empty());
        assert!(WindowsSearch::search("anything", 0).is_empty());
    }

    /// A fallback hit is from outside the chosen roots, so it must never outrank
    /// a local one. Pinned as a comparison rather than a magic number.
    #[test]
    fn v0_7_a_fallback_hit_scores_below_a_local_name_match() {
        let hit = hit_of(r"C:\Data\0Projects\Create\HH\bg.jpg".to_string()).unwrap();
        assert!(hit.score < rank::TIER_NAME_PREFIX);
        assert!(hit.score < rank::TIER_EXACT_NAME);
    }
}
