# Send a real keystroke, at the input layer.
#
# It has to be real input. `SendKeys` posts messages to the foreground window, and
# a global hotkey registered with `RegisterHotKey` is dispatched by the OS from the
# raw input stream -- so `SendKeys "% "` types a space into whatever is focused and
# never reaches Takyon. `keybd_event` injects at the same layer a keyboard does,
# which is the only way to exercise the code path being measured.
#
# The whole point of the harness is that the number describes the real hotkey path.
# A benchmark that quietly measured a synthetic show-window call would be the
# easiest possible way to produce four reassuring numbers that mean nothing.

param(
    [Parameter(Mandatory = $true)]
    [ValidateSet('AltSpace', 'CtrlAltF9', 'Escape', 'LetterC')]
    [string]$Key
)

Add-Type -Namespace Takyon -Name Input -MemberDefinition @'
    [DllImport("user32.dll", SetLastError = true)]
    public static extern void keybd_event(byte bVk, byte bScan, uint dwFlags, System.UIntPtr dwExtraInfo);
'@

$VK_CONTROL = 0x11
$VK_MENU = 0x12   # Alt
$VK_SPACE = 0x20
$VK_ESCAPE = 0x1B
$VK_F9 = 0x78
$VK_C = 0x43
$KEYEVENTF_KEYUP = 0x0002

switch ($Key) {
    'AltSpace' {
        [Takyon.Input]::keybd_event($VK_MENU, 0, 0, [UIntPtr]::Zero)
        [Takyon.Input]::keybd_event($VK_SPACE, 0, 0, [UIntPtr]::Zero)
        [Takyon.Input]::keybd_event($VK_SPACE, 0, $KEYEVENTF_KEYUP, [UIntPtr]::Zero)
        [Takyon.Input]::keybd_event($VK_MENU, 0, $KEYEVENTF_KEYUP, [UIntPtr]::Zero)
    }
    # The fallback chord, used when something else already owns Alt+Space --
    # which is most development machines, since PowerToys Run and Raycast both
    # claim it. Released in reverse order, as a keyboard would.
    'CtrlAltF9' {
        [Takyon.Input]::keybd_event($VK_CONTROL, 0, 0, [UIntPtr]::Zero)
        [Takyon.Input]::keybd_event($VK_MENU, 0, 0, [UIntPtr]::Zero)
        [Takyon.Input]::keybd_event($VK_F9, 0, 0, [UIntPtr]::Zero)
        [Takyon.Input]::keybd_event($VK_F9, 0, $KEYEVENTF_KEYUP, [UIntPtr]::Zero)
        [Takyon.Input]::keybd_event($VK_MENU, 0, $KEYEVENTF_KEYUP, [UIntPtr]::Zero)
        [Takyon.Input]::keybd_event($VK_CONTROL, 0, $KEYEVENTF_KEYUP, [UIntPtr]::Zero)
    }
    # One character into the focused Palette, for the "first Entry" budget.
    # `c` because it matches something on any Windows install -- Calculator,
    # Chrome, cmd, Control Panel -- so the measurement does not depend on what
    # happens to be installed on the machine running it.
    'LetterC' {
        [Takyon.Input]::keybd_event($VK_C, 0, 0, [UIntPtr]::Zero)
        [Takyon.Input]::keybd_event($VK_C, 0, $KEYEVENTF_KEYUP, [UIntPtr]::Zero)
    }
    'Escape' {
        [Takyon.Input]::keybd_event($VK_ESCAPE, 0, 0, [UIntPtr]::Zero)
        [Takyon.Input]::keybd_event($VK_ESCAPE, 0, $KEYEVENTF_KEYUP, [UIntPtr]::Zero)
    }
}
