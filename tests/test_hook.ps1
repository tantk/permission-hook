$json = '{"tool_name":"Read","tool_input":{"file_path":"test.txt"}}'
$pinfo = New-Object System.Diagnostics.ProcessStartInfo
$pinfo.FileName = 'C:\Users\tanti\.local\bin\claude-permission-hook.exe'
$pinfo.RedirectStandardInput = $true
$pinfo.RedirectStandardOutput = $true
$pinfo.RedirectStandardError = $true
$pinfo.UseShellExecute = $false

$proc = New-Object System.Diagnostics.Process
$proc.StartInfo = $pinfo
$proc.Start() | Out-Null
$proc.StandardInput.WriteLine($json)
$proc.StandardInput.Close()

$stdout = $proc.StandardOutput.ReadToEnd()
$stderr = $proc.StandardError.ReadToEnd()
$proc.WaitForExit()

Write-Host "Exit code: $($proc.ExitCode)"
Write-Host "STDOUT: [$stdout]"
Write-Host "STDERR: [$stderr]"
