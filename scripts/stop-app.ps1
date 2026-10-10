# Stop running app processes so release can overwrite the exe.
param([Parameter(Mandatory)][string[]]$Names)

$procs = Get-Process -Name $Names -ErrorAction SilentlyContinue
if (-not $procs) {
    Write-Host ("No running process: {0}" -f ($Names -join ', '))
    exit 0
}
foreach ($p in $procs) {
    Write-Host ("Stopping {0} (pid {1}) from {2}" -f $p.ProcessName, $p.Id, $p.Path)
}
$procs | Stop-Process -Force
Start-Sleep -Milliseconds 400
$left = Get-Process -Name $Names -ErrorAction SilentlyContinue
if ($left) {
    throw ("Still running after stop: " + (($left | ForEach-Object { "{0}:{1}" -f $_.ProcessName, $_.Id }) -join ', '))
}
Write-Host ('Stopped: ' + ($Names -join ', '))
