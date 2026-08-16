param(
  [Parameter(Mandatory = $true)][string]$ServerUrl,
  [Parameter(Mandatory = $true)][string]$BearerToken,
  [string]$EvidenceDir = "$env:USERPROFILE/AxonPaletteQA/evidence"
)

$ErrorActionPreference = "Stop"
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing
Add-Type @"
using System;
using System.Runtime.InteropServices;
public static class AxonQaMouse {
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
  [DllImport("user32.dll")] public static extern void mouse_event(uint flags, uint dx, uint dy, uint data, UIntPtr extra);
  [DllImport("user32.dll")] public static extern void keybd_event(byte key, byte scan, uint flags, UIntPtr extra);
  public static void Click(int x, int y) {
    SetCursorPos(x, y);
    mouse_event(0x0002, 0, 0, 0, UIntPtr.Zero);
    mouse_event(0x0004, 0, 0, 0, UIntPtr.Zero);
  }
  public static void ShowPalette() {
    keybd_event(0x11, 0, 0, UIntPtr.Zero);
    keybd_event(0x10, 0, 0, UIntPtr.Zero);
    keybd_event(0x20, 0, 0, UIntPtr.Zero);
    keybd_event(0x20, 0, 2, UIntPtr.Zero);
    keybd_event(0x10, 0, 2, UIntPtr.Zero);
    keybd_event(0x11, 0, 2, UIntPtr.Zero);
  }
}
"@

function Find-AxonElement([string]$name, [int]$timeoutSeconds = 10) {
  $deadline = (Get-Date).AddSeconds($timeoutSeconds)
  do {
    $root = [System.Windows.Automation.AutomationElement]::RootElement
    $condition = New-Object System.Windows.Automation.PropertyCondition(
      [System.Windows.Automation.AutomationElement]::NameProperty,
      $name
    )
    $element = $root.FindFirst(
      [System.Windows.Automation.TreeScope]::Descendants,
      $condition
    )
    if ($element) { return $element }
    Start-Sleep -Milliseconds 250
  } while ((Get-Date) -lt $deadline)
  throw "UI Automation element not found: $name"
}

function Invoke-AxonElement([string]$name) {
  $element = Find-AxonElement $name
  $pattern = $element.GetCurrentPattern(
    [System.Windows.Automation.InvokePattern]::Pattern
  )
  $pattern.Invoke()
}

function Invoke-AxonTrayIcon {
  $tray = Find-AxonElement "System tray overflow window."
  $condition = New-Object System.Windows.Automation.PropertyCondition(
    [System.Windows.Automation.AutomationElement]::NameProperty,
    "Axon Palette"
  )
  $element = $tray.FindFirst(
    [System.Windows.Automation.TreeScope]::Descendants,
    $condition
  )
  if (-not $element) { throw "Axon Palette tray icon not found" }
  $pattern = $element.GetCurrentPattern(
    [System.Windows.Automation.InvokePattern]::Pattern
  )
  $pattern.Invoke()
}

function Set-AxonValue([string]$name, [string]$value) {
  $element = Find-AxonElement $name
  $pattern = $element.GetCurrentPattern(
    [System.Windows.Automation.ValuePattern]::Pattern
  )
  $pattern.SetValue($value)
}

function Save-DesktopScreenshot([string]$path) {
  $bounds = [System.Windows.Forms.Screen]::PrimaryScreen.Bounds
  $bitmap = New-Object System.Drawing.Bitmap $bounds.Width, $bounds.Height
  $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
  try {
    $graphics.CopyFromScreen($bounds.Location, [System.Drawing.Point]::Empty, $bounds.Size)
    $bitmap.Save($path, [System.Drawing.Imaging.ImageFormat]::Png)
  } finally {
    $graphics.Dispose()
    $bitmap.Dispose()
  }
}

function Set-FocusedValue([int]$x, [int]$y, [string]$value) {
  [AxonQaMouse]::Click($x, $y)
  Start-Sleep -Milliseconds 150
  $script:shell.SendKeys("^a")
  # Send real key events so React's controlled input onChange handler receives
  # the edit. Setting ValuePattern/DOM value alone only changes the rendered
  # control and leaves the application state (and Save button) unchanged.
  $script:shell.SendKeys($value)
  Start-Sleep -Milliseconds 200
}

New-Item -ItemType Directory -Force $EvidenceDir | Out-Null
$shell = New-Object -ComObject WScript.Shell
# Show the palette through its real tray affordance. Keeping the full
# interaction in one process prevents hide-on-blur from racing between
# automation calls.
[AxonQaMouse]::Click(1733, 1056)
Start-Sleep -Milliseconds 400
Invoke-AxonTrayIcon
Start-Sleep -Seconds 1
[AxonQaMouse]::Click(1275, 516)
Start-Sleep -Milliseconds 300
[AxonQaMouse]::Click(1176, 380)
Start-Sleep -Milliseconds 500
Save-DesktopScreenshot "$EvidenceDir/01-first-run-settings.png"

Set-FocusedValue 765 407 $ServerUrl
Set-FocusedValue 761 482 $BearerToken
[AxonQaMouse]::Click(658, 745)
Start-Sleep -Seconds 12
Save-DesktopScreenshot "$EvidenceDir/02-connection-test.png"
[AxonQaMouse]::Click(1306, 745)
Start-Sleep -Seconds 1

[pscustomobject]@{
  server = $ServerUrl
  settings_saved = $true
  token_length = $BearerToken.Length
  evidence = $EvidenceDir
} | ConvertTo-Json
