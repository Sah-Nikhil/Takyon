//! Processes Focus never quits, whatever a Block list says.
//!
//! One list, here. The Allow list (designed, not built) reuses it as its
//! always-allowed tier.

use std::path::Path;

use super::identity::{norm, Process, Target};

/// Whether quitting `process` is off limits. `takyon_dir`: Takyon's install
/// directory, which holds the app and its UIAccess helper.
pub fn is_protected(process: &Process, takyon_dir: &Path) -> bool {
    let image = norm(&process.image);
    process.session_id == 0
        || PROTECTED.contains(&file_name(&image))
        || under(&image, &norm(takyon_dir))
}

/// Whether a Block list item would hit something protected, so the start
/// form refuses it in words instead of filtering it quietly.
pub fn refuses(target: &Target, takyon_dir: &Path) -> bool {
    let takyon = norm(takyon_dir);
    match target {
        Target::Path(path) => PROTECTED.contains(&file_name(path)) || under(path, &takyon),
        Target::Dir(dir) => under(&takyon, dir) || under(dir, &takyon),
        Target::ExeName(name) => PROTECTED.contains(&name.as_str()),
        Target::Package(_) | Target::Aumid(_) => false,
    }
}

/// Image names, lowercase. Shell, input (IMEs, touch keyboard), the UWP frame
/// host every Store window lives in, Task Manager, Takyon's own two.
const PROTECTED: &[&str] = &[
    "explorer.exe",
    "shellexperiencehost.exe",
    "startmenuexperiencehost.exe",
    "searchhost.exe",
    "textinputhost.exe",
    "ctfmon.exe",
    "applicationframehost.exe",
    "sihost.exe",
    "lockapp.exe",
    "dwm.exe",
    "csrss.exe",
    "winlogon.exe",
    "taskmgr.exe",
    "takyon.exe",
    "takyon-uiaccess-helper.exe",
];

fn file_name(path: &str) -> &str {
    path.rsplit('\\').next().unwrap_or(path)
}

/// `path` is `dir` or inside it.
fn under(path: &str, dir: &str) -> bool {
    let dir = dir.trim_end_matches('\\');
    path.strip_prefix(dir)
        .is_some_and(|rest| rest.is_empty() || rest.starts_with('\\'))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    const TAKYON: &str = r"D:\Takyon";

    fn exe(path: &str, session_id: u32) -> Process {
        Process {
            image: PathBuf::from(path),
            session_id,
            ..Default::default()
        }
    }

    #[test]
    fn v0_16_the_shell_and_input_stack_are_never_quit() {
        for image in [
            r"C:\Windows\explorer.exe",
            r"C:\Windows\SystemApps\ShellExperienceHost_cw5n1h2txyewy\ShellExperienceHost.exe",
            r"C:\Windows\SystemApps\MicrosoftWindows.Client.CBS_cw5n1h2txyewy\TextInputHost.exe",
            r"C:\Windows\System32\ctfmon.exe",
            r"C:\Windows\System32\ApplicationFrameHost.exe",
            r"C:\Windows\System32\Taskmgr.exe",
            r"C:\Windows\System32\dwm.exe",
        ] {
            assert!(is_protected(&exe(image, 1), Path::new(TAKYON)), "{image}");
        }
        assert!(!is_protected(
            &exe(r"D:\Apps\Discord\Discord.exe", 1),
            Path::new(TAKYON)
        ));
    }

    #[test]
    fn v0_16_takyon_and_services_are_never_quit() {
        assert!(is_protected(
            &exe(r"D:\Takyon\takyon.exe", 1),
            Path::new(TAKYON)
        ));
        assert!(is_protected(
            &exe(r"d:\takyon\uiaccess\takyon-uiaccess.exe", 1),
            Path::new(TAKYON)
        ));
        assert!(is_protected(
            &exe(r"C:\Program Files\Some\service.exe", 0),
            Path::new(TAKYON)
        ));
        assert!(!is_protected(
            &exe(r"D:\TakyonOther\x.exe", 1),
            Path::new(TAKYON)
        ));
    }

    #[test]
    fn v0_16_a_block_list_naming_takyon_or_the_shell_is_refused() {
        let takyon = Path::new(TAKYON);
        assert!(refuses(
            &Target::Path(r"d:\takyon\takyon.exe".into()),
            takyon
        ));
        assert!(refuses(&Target::Dir(r"d:\takyon".into()), takyon));
        assert!(refuses(&Target::Dir(r"d:\".into()), takyon));
        assert!(refuses(&Target::ExeName("explorer.exe".into()), takyon));
        assert!(refuses(&Target::ExeName("takyon.exe".into()), takyon));
        assert!(refuses(
            &Target::Path(r"c:\windows\explorer.exe".into()),
            takyon
        ));
        assert!(!refuses(&Target::ExeName("discord.exe".into()), takyon));
        assert!(!refuses(
            &Target::Dir(r"d:\steam\steamapps\common\game".into()),
            takyon
        ));
    }
}
