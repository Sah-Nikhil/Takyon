; Remove the login registration on uninstall.
;
; `auto-launch` writes TWO values, and deleting only the first leaves Windows with
; an approval record for an app that no longer exists:
;   HKCU\...\CurrentVersion\Run                        -> the command line
;   HKCU\...\Explorer\StartupApproved\Run              -> the enabled/disabled flag
;
; Both are keyed by the name passed to `Builder::app_name()`, which per ADR-0020 is
; the slug `com.v3sper.takyon`. The `com.v3sper.launcher` pair below is the
; pre-rename name: a machine that ran an older build still carries it, and an
; uninstall that skips it leaves an orphan pointing at a deleted binary forever.

; Drop the pre-rename autostart value on upgrade.
;
; The new build registers under `com.v3sper.takyon` and never touches the old
; name, so without this an upgrading machine carries two Run values. The second
; one still resolves — same install path — so single-instance hides it and the
; orphan only surfaces after an uninstall.
!macro NSIS_HOOK_POSTINSTALL
  DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "com.v3sper.launcher"
  DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run" "com.v3sper.launcher"
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "com.v3sper.takyon"
  DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run" "com.v3sper.takyon"
  DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "com.v3sper.launcher"
  DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run" "com.v3sper.launcher"
!macroend
