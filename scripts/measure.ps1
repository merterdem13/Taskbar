param(
    [Parameter(Mandatory = $false)]
    [string]$ExePath = (Join-Path $PSScriptRoot "..\target\release\bar.exe"),
    [Parameter(Mandatory = $false)]
    [int]$DurationSeconds = 12,
    [Parameter(Mandatory = $false)]
    [int]$IntervalMs = 1000
)

$ErrorActionPreference = "Stop"

$resolvedExe = (Resolve-Path $ExePath).Path

Add-Type @"
using System;
using System.Runtime.InteropServices;

public static class Win32Close {
    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    public static extern IntPtr FindWindowW(string lpClassName, string lpWindowName);

    [DllImport("user32.dll")]
    public static extern bool PostMessageW(IntPtr hWnd, uint Msg, IntPtr wParam, IntPtr lParam);
}
"@

$proc = Start-Process -FilePath $resolvedExe -PassThru
try {
    Start-Sleep -Milliseconds 1500
    $proc.Refresh()
    $startCpuMs = $proc.TotalProcessorTime.TotalMilliseconds

    $samples = @()
    $iterations = [math]::Max(1, [int](($DurationSeconds * 1000) / $IntervalMs))
    for ($i = 0; $i -lt $iterations; $i++) {
        Start-Sleep -Milliseconds $IntervalMs
        if ($proc.HasExited) {
            break
        }
        $proc.Refresh()
        $wsMb = [math]::Round($proc.WorkingSet64 / 1MB, 2)
        $privMb = [math]::Round($proc.PrivateMemorySize64 / 1MB, 2)
        $cpuMs = [math]::Round($proc.TotalProcessorTime.TotalMilliseconds, 2)
        $samples += [PSCustomObject]@{
            Sample       = $i + 1
            WorkingSetMB = $wsMb
            PrivateMB    = $privMb
            TotalCpuMs   = $cpuMs
        }
    }

    $proc.Refresh()
    $endCpuMs = $proc.TotalProcessorTime.TotalMilliseconds
    $idleCpuDeltaMs = [math]::Round($endCpuMs - $startCpuMs, 2)

    $avgWs = [math]::Round(($samples | Measure-Object -Property WorkingSetMB -Average).Average, 2)
    $maxWs = [math]::Round(($samples | Measure-Object -Property WorkingSetMB -Maximum).Maximum, 2)
    $avgPriv = [math]::Round(($samples | Measure-Object -Property PrivateMB -Average).Average, 2)
    $maxPriv = [math]::Round(($samples | Measure-Object -Property PrivateMB -Maximum).Maximum, 2)
    $startupCpuMs = [math]::Round($startCpuMs, 2)

    Write-Output "### Measurement Summary (`"$(Split-Path $resolvedExe -Leaf)`")"
    Write-Output ""
    Write-Output "- **Duration:** ${DurationSeconds}s (${iterations} samples @ ${IntervalMs}ms)"
    Write-Output "- **Working Set (avg / max):** ${avgWs} MB / ${maxWs} MB"
    Write-Output "- **Private Bytes (avg / max):** ${avgPriv} MB / ${maxPriv} MB"
    Write-Output "- **Startup CPU time:** ${startupCpuMs} ms"
    Write-Output "- **Idle CPU delta over ${DurationSeconds}s:** ${idleCpuDeltaMs} ms"
    Write-Output ""
    Write-Output "| Sample | Working Set (MB) | Private Bytes (MB) | Total CPU (ms) |"
    Write-Output "|---:|---:|---:|---:|"
    foreach ($s in $samples) {
        Write-Output "| $($s.Sample) | $($s.WorkingSetMB) | $($s.PrivateMB) | $($s.TotalCpuMs) |"
    }
}
finally {
    if (-not $proc.HasExited) {
        $hwnd = [Win32Close]::FindWindowW("RustTaskbarWindow", $null)
        if ($hwnd -ne [IntPtr]::Zero) {
            [void][Win32Close]::PostMessageW($hwnd, 0x0010, [IntPtr]::Zero, [IntPtr]::Zero)
            [void]$proc.WaitForExit(3000)
        }
        if (-not $proc.HasExited) {
            Stop-Process -Id $proc.Id -Force -ErrorAction SilentlyContinue
        }
    }
}
