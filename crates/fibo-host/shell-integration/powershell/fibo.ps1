# Fibo PowerShell integration. Fibo starts an interactive PowerShell with -NoExit -Command
# running this file once the user's profile has run, so it wraps whatever prompt the profile
# left: each prompt first reports the working directory with OSC 7 (file://host/path,
# percent-encoded UTF-8), so the pane's place, new tabs and splits follow `cd` as they do on
# a Mac. PowerShell keeps its location to itself (the process directory never moves), so
# this report is the only way anyone outside learns it. Read as text rather than run as a
# script file, so an execution policy that forbids scripts does not stop it.
if (-not (Test-Path variable:global:__FiboPrompt)) {
    $global:__FiboPrompt = $function:prompt
    function global:prompt {
        $fiboOk = $?
        $fiboExit = $global:LASTEXITCODE
        $report = ''
        $here = $ExecutionContext.SessionState.Path.CurrentLocation
        # Only a directory on this machine: not the registry, not a share on another one.
        if ($here.Provider.Name -eq 'FileSystem' -and $here.ProviderPath -match '^([A-Za-z]:|/)') {
            $path = $here.ProviderPath -replace '\\', '/'
            if ($path -notmatch '^/') { $path = '/' + $path }
            # Byte by byte, so a path in any language encodes as its UTF-8 bytes.
            $out = New-Object System.Text.StringBuilder
            foreach ($b in [System.Text.Encoding]::UTF8.GetBytes($path)) {
                if ($b -lt 128 -and ([char]$b) -match '[-/._~:A-Za-z0-9]') {
                    [void]$out.Append([char]$b)
                } else {
                    [void]$out.AppendFormat('%{0:X2}', $b)
                }
            }
            $e = [char]27
            $report = "$e]7;file://$([Environment]::MachineName)$out$e\"
        }
        # The prompt the user had sees the command's own status, not this function's.
        $global:LASTEXITCODE = $fiboExit
        if (-not $fiboOk) { Write-Error 'failed' -ErrorAction Ignore }
        $shown = if ($global:__FiboPrompt) { & $global:__FiboPrompt } else { "PS $here> " }
        $report + $shown
    }
}
