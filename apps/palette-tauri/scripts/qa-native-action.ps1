param(
  [Parameter(Mandatory = $true)][string]$Command,
  [Parameter(Mandatory = $true)][string]$ScreenshotPath,
  [int]$WaitSeconds = 12
)

$ErrorActionPreference = "Stop"
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing
Add-Type @"
using System;
using System.Runtime.InteropServices;
public static class AxonActionMouse {
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
  [DllImport("user32.dll")] public static extern void mouse_event(uint flags, uint dx, uint dy, uint data, UIntPtr extra);
  public static void Click(int x, int y) {
    SetCursorPos(x, y); mouse_event(2,0,0,0,UIntPtr.Zero); mouse_event(4,0,0,0,UIntPtr.Zero);
  }
}
"@

function Find-Named([System.Windows.Automation.AutomationElement]$root, [string]$name) {
  $condition = New-Object System.Windows.Automation.PropertyCondition(
    [System.Windows.Automation.AutomationElement]::NameProperty, $name
  )
  $root.FindFirst([System.Windows.Automation.TreeScope]::Descendants, $condition)
}

function Show-Palette {
  $root = [System.Windows.Automation.AutomationElement]::RootElement
  $commandInput = Find-Named $root "Axon command"
  if ($commandInput) { return }
  $hidden = Find-Named $root "Show Hidden Icons"
  $tray = Find-Named $root "System tray overflow window."
  if (-not $tray) {
    if (-not $hidden) { $hidden = Find-Named $root "Show Hidden Icons Hide" }
    if (-not $hidden) { throw "Show Hidden Icons button not found" }
    $hidden.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke()
    Start-Sleep -Milliseconds 400
    $tray = Find-Named $root "System tray overflow window."
  }
  if (-not $tray) { throw "system tray overflow not found" }
  $icon = Find-Named $tray "Axon Palette"
  if (-not $icon) { throw "Axon Palette tray icon not found" }
  $rect = $icon.Current.BoundingRectangle
  [AxonActionMouse]::Click([int]($rect.X + $rect.Width / 2), [int]($rect.Y + $rect.Height / 2))
  $deadline = (Get-Date).AddSeconds(8)
  do {
    Start-Sleep -Milliseconds 250
    $commandInput = Find-Named $root "Axon command"
  } while (-not $commandInput -and (Get-Date) -lt $deadline)
  if (-not $commandInput) { throw "Axon command input did not appear" }
}

function Save-Screenshot([string]$path) {
  $parent = Split-Path -Parent $path
  New-Item -ItemType Directory -Force $parent | Out-Null
  $bounds = [System.Windows.Forms.Screen]::PrimaryScreen.Bounds
  $bitmap = New-Object System.Drawing.Bitmap $bounds.Width, $bounds.Height
  $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
  try {
    $graphics.CopyFromScreen($bounds.Location, [System.Drawing.Point]::Empty, $bounds.Size)
    $bitmap.Save($path, [System.Drawing.Imaging.ImageFormat]::Png)
  } finally { $graphics.Dispose(); $bitmap.Dispose() }
}

Show-Palette
$root = [System.Windows.Automation.AutomationElement]::RootElement
$reset = Find-Named $root "Reset Axon palette"
if ($reset) {
  $resetRect = $reset.Current.BoundingRectangle
  [AxonActionMouse]::Click([int]($resetRect.X + $resetRect.Width / 2), [int]($resetRect.Y + $resetRect.Height / 2))
  Start-Sleep -Milliseconds 300
}
$commandInput = Find-Named $root "Axon command"
if (-not $commandInput) { throw "Axon command input not available after reset" }
$inputRect = $commandInput.Current.BoundingRectangle
[AxonActionMouse]::Click([int]($inputRect.X + $inputRect.Width / 2), [int]($inputRect.Y + $inputRect.Height / 2))
$shell = New-Object -ComObject WScript.Shell
$shell.SendKeys("^a")
$shell.SendKeys($Command)
$shell.SendKeys("{ENTER}")
Start-Sleep -Seconds $WaitSeconds
Save-Screenshot $ScreenshotPath
[pscustomobject]@{ command = $Command; screenshot = $ScreenshotPath } | ConvertTo-Json
