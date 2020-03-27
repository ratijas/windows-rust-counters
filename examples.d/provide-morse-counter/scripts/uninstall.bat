@echo off
echo Make sure you are running this script with admin privileges

echo[
echo Uninstalling counters
unlodctr Morse
echo]

echo[
echo Uninstalling from registry
reg delete HKEY_LOCAL_MACHINE\SYSTEM\CurrentControlSet\Services\Morse\                     /f
reg delete HKEY_LOCAL_MACHINE\SYSTEM\CurrentControlSet\Services\EventLog\Application\Morse /f
echo]

echo[
echo Removing dll from System32 directory
del "C:\Windows\System32\ExampleProvideMorseCounter.dll"
echo]

echo[
echo Done
echo]
