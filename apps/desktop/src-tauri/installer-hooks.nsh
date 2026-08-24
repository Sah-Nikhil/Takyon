; Remove the login registration on uninstall.
;
; `auto-launch` writes TWO values, and deleting only the first leaves Windows with
; an approval record for an app that no longer exists:
;   HKCU\...\CurrentVersion\Run                        -> the command line
;   HKCU\...\Explorer\StartupApproved\Run              -> the enabled/disabled flag
;
; Both are keyed by the name passed to `Builder::app_name()`, which per ADR-0011 is
; the neutral slug `com.v3sper.launcher` and NOT the display name. Renaming the
; product must not orphan this key, which is the entire point of the separation —
; so the literal below is the slug, deliberately.

!macro NSIS_HOOK_POSTUNINSTALL
  DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "com.v3sper.launcher"
  DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run" "com.v3sper.launcher"
!macroend
