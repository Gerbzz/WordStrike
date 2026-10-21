param(
    [Parameter(Mandatory = $true)][string]$Executable,
    [Parameter(Mandatory = $true)][string]$OutputDirectory,
    [ValidateRange(15, 120)][int]$StartupTimeoutSeconds = 90
)

# CI-only capture of the real app and its Windows title bar. No raster edits or overlays.
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
if (-not $IsWindows) { throw 'This native window capture requires Windows.' }
if (-not $env:RUNNER_TEMP) { throw 'Use the isolated hosted runner temporary directory.' }
$repositoryRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$executablePath = (Resolve-Path -LiteralPath $Executable).Path
$runnerRoot = [IO.Path]::GetFullPath($env:RUNNER_TEMP).TrimEnd('\', '/') + [IO.Path]::DirectorySeparatorChar
$outputPath = [IO.Path]::GetFullPath($OutputDirectory)
if (-not $outputPath.StartsWith($runnerRoot, [StringComparison]::OrdinalIgnoreCase)) {
    throw 'Capture output must be inside RUNNER_TEMP.'
}
if (Test-Path -LiteralPath $outputPath) { throw 'Use a fresh capture output directory.' }
$null = New-Item -ItemType Directory -Path $outputPath
$privateRoot = Join-Path $runnerRoot ('wordstrike-capture-profile-' + [Guid]::NewGuid().ToString('N'))
$null = New-Item -ItemType Directory -Path $privateRoot
$roamingPath = Join-Path $privateRoot 'roaming'
$localPath = Join-Path $privateRoot 'local'
$null = New-Item -ItemType Directory -Path $roamingPath, $localPath
$fixturePath = Join-Path $repositoryRoot 'fixtures/wordstrike/with-image.docx'
$fixtureHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $fixturePath).Hash
$samplePath = Join-Path $privateRoot 'WordStrike sample.docx'
Copy-Item -LiteralPath $fixturePath -Destination $samplePath
$outerPng = Join-Path $outputPath 'wordstrike-native-window.png'
$clientPng = Join-Path $outputPath 'wordstrike-native-client.png'

Add-Type -AssemblyName System.Windows.Forms, System.Drawing
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
using System.Text;
public static class WordStrikeCaptureWindow {
    [StructLayout(LayoutKind.Sequential)]
    public struct RECT { public int Left, Top, Right, Bottom; }
    [DllImport("user32.dll")] public static extern IntPtr SetThreadDpiAwarenessContext(IntPtr context);
    [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr window);
    [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr window, out uint processId);
    [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr window);
    [DllImport("user32.dll")] public static extern bool ShowWindowAsync(IntPtr window, int command);
    [DllImport("user32.dll")] public static extern bool MoveWindow(IntPtr window, int x, int y, int width, int height, bool repaint);
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr window, out RECT rect);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern int GetWindowText(IntPtr window, StringBuilder text, int length);
    [DllImport("user32.dll", EntryPoint = "GetWindowLongW")] public static extern int GetWindowStyle(IntPtr window, int index);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern IntPtr SendMessage(IntPtr window, uint message, IntPtr wParam, IntPtr lParam);
}
'@

$appProcess = $null
$controlKey = $null
$previousDpiContext = [WordStrikeCaptureWindow]::SetThreadDpiAwarenessContext([IntPtr]::new(-4))
try {
    # Let Windows choose a free loopback port. The app itself creates a fresh per-port key.
    $listener = [Net.Sockets.TcpListener]::new([Net.IPAddress]::Loopback, 0)
    $listener.Start()
    $controlPort = $listener.LocalEndpoint.Port
    $listener.Stop()
    $keyPath = Join-Path $roamingPath "WordStrike/control-key.local-$controlPort"
    $startInfo = [Diagnostics.ProcessStartInfo]::new($executablePath)
    $startInfo.UseShellExecute = $false
    $startInfo.WorkingDirectory = $privateRoot
    $startInfo.Environment['APPDATA'] = $roamingPath
    $startInfo.Environment['LOCALAPPDATA'] = $localPath
    $startInfo.Environment['WORDCRAFT_NO_PREFS'] = '1'
    $startInfo.RedirectStandardOutput = $true
    $startInfo.RedirectStandardError = $true
    $startInfo.ArgumentList.Add('--control')
    $startInfo.ArgumentList.Add([string]$controlPort)
    $startInfo.ArgumentList.Add($samplePath)
    $appProcess = [Diagnostics.Process]::Start($startInfo)
    $stdoutTask = $appProcess.StandardOutput.ReadToEndAsync()
    $stderrTask = $appProcess.StandardError.ReadToEndAsync()
    $deadline = [DateTime]::UtcNow.AddSeconds($StartupTimeoutSeconds)
    do {
        if ($appProcess.HasExited) { throw 'WordStrike exited before its control channel was ready.' }
        if (Test-Path -LiteralPath $keyPath) {
            $controlKey = [IO.File]::ReadAllText($keyPath).Trim()
            if ($controlKey -match '^[0-9a-f]{64}$') { break }
        }
        Start-Sleep -Milliseconds 200
    } while ([DateTime]::UtcNow -lt $deadline)
    if (-not $controlKey -or $controlKey -notmatch '^[0-9a-f]{64}$') {
        throw 'WordStrike did not create its isolated control key.'
    }

    function Invoke-WordStrikeControl([string]$Method, [hashtable]$Parameters = @{}) {
        $client = [Net.Sockets.TcpClient]::new()
        try {
            if (-not $client.ConnectAsync([Net.IPAddress]::Loopback, $controlPort).Wait(5000)) {
                throw 'WordStrike loopback connection timed out.'
            }
            $stream = $client.GetStream()
            $stream.ReadTimeout = 20000
            $stream.WriteTimeout = 5000
            $writer = [IO.StreamWriter]::new($stream, [Text.UTF8Encoding]::new($false), 4096, $true)
            $reader = [IO.StreamReader]::new($stream, [Text.UTF8Encoding]::new($false), $false, 4096, $true)
            try {
                # Never log or save the request: it contains the ephemeral control key.
                $request = @{ id = 1; key = $controlKey; method = $Method; params = $Parameters }
                $writer.WriteLine(($request | ConvertTo-Json -Compress -Depth 10))
                $writer.Flush()
                $line = $reader.ReadLine()
                if (-not $line) { throw "WordStrike returned no reply to $Method." }
                $reply = $line | ConvertFrom-Json -AsHashtable
                if (-not $reply.ContainsKey('ok') -or -not $reply.ok) {
                    throw "WordStrike rejected $Method."
                }
                return $reply.result
            } finally {
                $reader.Dispose()
                $writer.Dispose()
            }
        } finally { $client.Dispose() }
    }

    # This is read-only inspection of the self-authored fixture, not a document edit.
    $document = Invoke-WordStrikeControl 'document.inspect' @{ text = $true }
    $documentText = ($document.blocks | ForEach-Object { if ($_.ContainsKey('text')) { $_.text } }) -join "`n"
    if ($documentText -notmatch 'Quarterly Field Notes' -or $document.pages -lt 1) {
        throw 'The expected public sample document was not loaded.'
    }
    $ui = Invoke-WordStrikeControl 'ui.inspect'
    if ($ui.dirty -or $null -ne $ui.dialog) { throw 'The sample is dirty or a dialog obscures it.' }

    $deadline = [DateTime]::UtcNow.AddSeconds(15)
    do {
        $appProcess.Refresh()
        $window = $appProcess.MainWindowHandle
        if ($window -ne [IntPtr]::Zero -and [WordStrikeCaptureWindow]::IsWindowVisible($window)) { break }
        Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $deadline)
    if ($window -eq [IntPtr]::Zero) { throw 'No native WordStrike window appeared.' }
    [uint32]$windowProcessId = 0
    $null = [WordStrikeCaptureWindow]::GetWindowThreadProcessId($window, [ref]$windowProcessId)
    if ($windowProcessId -ne $appProcess.Id) { throw 'The window does not belong to the spawned app.' }
    $title = [Text.StringBuilder]::new(1024)
    $null = [WordStrikeCaptureWindow]::GetWindowText($window, $title, $title.Capacity)
    if ($title.ToString() -notmatch 'WordStrike') { throw 'The native window title is not WordStrike.' }
    if (([WordStrikeCaptureWindow]::GetWindowStyle($window, -16) -band 0x00C00000) -ne 0x00C00000) {
        throw 'The actual Windows caption is absent; a client-only capture is insufficient.'
    }
    $smallIcon = [WordStrikeCaptureWindow]::SendMessage($window, 0x007F, [IntPtr]::Zero, [IntPtr]::Zero)
    if ($smallIcon -eq [IntPtr]::Zero) {
        $smallIcon = [WordStrikeCaptureWindow]::SendMessage($window, 0x007F, [IntPtr]::new(2), [IntPtr]::Zero)
    }
    if ($smallIcon -eq [IntPtr]::Zero) { throw 'The native window has no small icon to inspect.' }
    $workArea = [Windows.Forms.Screen]::PrimaryScreen.WorkingArea
    $width = [Math]::Min(1280, $workArea.Width - 48)
    $height = [Math]::Min(900, $workArea.Height - 48)
    if ($width -lt 960 -or $height -lt 640) { throw 'The runner display is too small for a readable capture.' }
    $null = [WordStrikeCaptureWindow]::ShowWindowAsync($window, 9)
    if (-not [WordStrikeCaptureWindow]::MoveWindow($window, $workArea.Left + 24, $workArea.Top + 24, $width, $height, $true)) {
        throw 'Cannot place the native window within the runner display.'
    }
    $null = [WordStrikeCaptureWindow]::SetForegroundWindow($window)
    $deadline = [DateTime]::UtcNow.AddSeconds(5)
    while ([WordStrikeCaptureWindow]::GetForegroundWindow() -ne $window -and [DateTime]::UtcNow -lt $deadline) {
        Start-Sleep -Milliseconds 100
    }
    if ([WordStrikeCaptureWindow]::GetForegroundWindow() -ne $window) { throw 'WordStrike is not the foreground window.' }
    $clientCapture = Invoke-WordStrikeControl 'ui.screenshot' @{ path = $clientPng }
    if ($clientCapture.width -lt 900 -or $clientCapture.height -lt 500) { throw 'The client screenshot is incomplete.' }
    Start-Sleep -Milliseconds 500
    $rectangle = [WordStrikeCaptureWindow+RECT]::new()
    if (-not [WordStrikeCaptureWindow]::GetWindowRect($window, [ref]$rectangle)) { throw 'Cannot inspect the outer window.' }
    $null = [WordStrikeCaptureWindow]::GetWindowThreadProcessId($window, [ref]$windowProcessId)
    if ($windowProcessId -ne $appProcess.Id -or [WordStrikeCaptureWindow]::GetForegroundWindow() -ne $window) {
        throw 'The verified app lost foreground ownership before capture.'
    }
    if ($rectangle.Left -lt $workArea.Left -or $rectangle.Top -lt $workArea.Top -or
        $rectangle.Right -gt $workArea.Right -or $rectangle.Bottom -gt $workArea.Bottom) {
        throw 'The entire app window must fit inside the runner display.'
    }
    $bitmap = [Drawing.Bitmap]::new($rectangle.Right - $rectangle.Left, $rectangle.Bottom - $rectangle.Top)
    try {
        $graphics = [Drawing.Graphics]::FromImage($bitmap)
        try {
            $graphics.CopyFromScreen($rectangle.Left, $rectangle.Top, 0, 0, $bitmap.Size, [Drawing.CopyPixelOperation]::SourceCopy)
        } finally { $graphics.Dispose() }
        # Save the original captured pixels: no logo substitution, annotation, crop or rescale.
        $bitmap.Save($outerPng, [Drawing.Imaging.ImageFormat]::Png)
        $outerWidth = $bitmap.Width
        $outerHeight = $bitmap.Height
    } finally { $bitmap.Dispose() }
    if ([WordStrikeCaptureWindow]::GetForegroundWindow() -ne $window) { throw 'The app lost foreground during capture.' }
    $null = Invoke-WordStrikeControl 'app.quit'
    if (-not $appProcess.WaitForExit(15000) -or $appProcess.ExitCode -ne 0) { throw 'WordStrike did not exit cleanly.' }
    if ((Get-FileHash -Algorithm SHA256 -LiteralPath $fixturePath).Hash -ne $fixtureHash -or
        (Get-FileHash -Algorithm SHA256 -LiteralPath $samplePath).Hash -ne $fixtureHash) {
        throw 'The original fixture or its temporary copy changed.'
    }
    $sourceRevision = (& git -C $repositoryRoot rev-parse HEAD).Trim()
    if ($LASTEXITCODE -ne 0) { throw 'Cannot record the actual checkout revision.' }
    $sourceTree = (& git -C $repositoryRoot rev-parse 'HEAD^{tree}').Trim()
    if ($LASTEXITCODE -ne 0) { throw 'Cannot record the actual source tree.' }
    $proof = [ordered]@{
        status = 'captured-for-independent-visual-review'
        sourceRevision = $sourceRevision
        sourceTree = $sourceTree
        pullRequestHead = $env:WORDSTRIKE_CAPTURE_HEAD
        runId = $env:GITHUB_RUN_ID
        capturedAtUtc = [DateTime]::UtcNow.ToString('o')
        executableSha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $executablePath).Hash
        publicFixture = 'fixtures/wordstrike/with-image.docx'
        fixtureSha256 = $fixtureHash
        fixtureAndCopyUnchanged = $true
        sourceIcon = 'native/wordstrike/assets/app-icon/wordstrike-geometric-256.png'
        sourceIconSha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath (Join-Path $repositoryRoot 'native/wordstrike/assets/app-icon/wordstrike-geometric-256.png')).Hash
        spawnedProcessId = $appProcess.Id
        capturedWindowProcessId = $windowProcessId
        foregroundVerified = $true
        windowsCaptionPresent = $true
        nativeSmallIconPresent = $true
        title = $title.ToString()
        documentPages = $document.pages
        expectedSampleTextPresent = $true
        outerWindow = @{ file = [IO.Path]::GetFileName($outerPng); width = $outerWidth; height = $outerHeight; sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $outerPng).Hash }
        client = @{ file = [IO.Path]::GetFileName($clientPng); width = $clientCapture.width; height = $clientCapture.height; sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $clientPng).Hash }
        rasterEdits = $false
        limits = 'Hosted synthetic-document capture only; logo appearance still requires visual review. Not Room acceptance, taskbar proof, seven-payload PE verification, or a release.'
    }
    $proof | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath (Join-Path $outputPath 'proof.json') -Encoding utf8
    Write-Output 'Captured the actual WordStrike outer window and client for independent visual review.'
} finally {
    if ($null -ne $appProcess -and -not $appProcess.HasExited) {
        $appProcess.Kill()
        $null = $appProcess.WaitForExit(5000)
    }
    if ($null -ne $appProcess) { $appProcess.Dispose() }
    $controlKey = $null
    if ($previousDpiContext -ne [IntPtr]::Zero) {
        $null = [WordStrikeCaptureWindow]::SetThreadDpiAwarenessContext($previousDpiContext)
    }
    # The private profile, key and process logs are never uploaded; the hosted runner discards them.
}
