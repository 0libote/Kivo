!include "WinVer.nsh"
!include "LogicLib.nsh"

!macro NSIS_HOOK_PREINSTALL
  ${IfNot} ${AtLeastBuild} 26100
    MessageBox MB_OK|MB_ICONSTOP "Kivo requires Windows 11 24H2 or later."
    Abort
  ${EndIf}
!macroend
