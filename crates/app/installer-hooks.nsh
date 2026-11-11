; 새 버전을 깔기 전에 예전 ffmpeg DLL·조건 글을 지움: 덮어 깔면 지금 안 쓰는 예전 부품(0.1.0 은 DLL 114개)이 남고, 지울 때도 안 지워짐.
; 지금 버전 것은 이 뒤에 다시 복사됨. 우리 앱이 깔린 폴더일 때만 (설치 폴더를 바꿨을 때 남의 DLL 을 지우지 않게).
!macro NSIS_HOOK_PREINSTALL
  ${If} ${FileExists} "$INSTDIR\${MAINBINARYNAME}.exe"
    Delete "$INSTDIR\*.dll"
    RMDir /r "$INSTDIR\licenses"
  ${EndIf}
!macroend
