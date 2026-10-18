#define MyAppName "姹叉箥 (JiTuan)"
#define MyAppVersion "0.2.2"
#define MyAppPublisher "choked-up-now"
#define MyAppURL "https://atomgit.com/choked-up-now/JiTuan"

[Setup]
AppId={{D3C9F88A-7F1B-4C1A-9B2A-8A2C5F6D7E8B}
AppName={#MyAppName}
AppVersion={#MyAppVersion}
AppPublisher={#MyAppPublisher}
AppPublisherURL={#MyAppURL}
AppSupportURL={#MyAppURL}
DefaultDirName={autopf}\JiTuan
DefaultGroupName={#MyAppName}
AllowNoIcons=yes
OutputDir=Output
OutputBaseFilename=JiTuan_v{#MyAppVersion}_Setup
Compression=lzma
SolidCompression=yes
WizardStyle=modern
PrivilegesRequired=admin
CloseApplications=yes
RestartApplications=no

[Languages]
Name: "chinesesimplified"; MessagesFile: "compiler:Languages\ChineseSimplified.isl"

[Tasks]
Name: "desktopicon"; Description: "鍒涘缓妗岄潰蹇嵎鏂瑰紡"; GroupDescription: "闄勫姞鍥炬爣:"

[Files]
Source: "JiTuan_Release\jituan-gui.exe"; DestDir: "{app}"; Components: gui
Source: "JiTuan_Release\jituan-cli.exe"; DestDir: "{app}"; Components: cli
Source: "JiTuan_Release\LICENSE"; DestDir: "{app}"; Components: gui or cli
Source: "JiTuan_Release\NOTICE"; DestDir: "{app}"; Components: gui or cli

[Components]
; 涓よ€呴兘鍙€夛紝浣嗛渶瑕侀€氳繃浠ｇ爜鎷︽埅
Name: "gui"; Description: "姹叉箥 鍥惧舰鐣岄潰鐗?(GUI)"; Types: full compact custom
Name: "cli"; Description: "姹叉箥 鍛戒护琛岀増 (CLI, 灏嗚嚜鍔ㄥ姞鍏ョ郴缁?PATH)"; Types: full custom

[Icons]
Name: "{group}\姹叉箥"; Filename: "{app}\jituan-gui.exe"; Components: gui
Name: "{group}\鍗歌浇 姹叉箥"; Filename: "{uninstallexe}"
Name: "{autodesktop}\姹叉箥"; Filename: "{app}\jituan-gui.exe"; Tasks: desktopicon; Components: gui

[Run]
Filename: "{app}\jituan-gui.exe"; Description: "绔嬪嵆杩愯 姹叉箥"; Flags: nowait postinstall skipifsilent; Components: gui

[Code]
function NextButtonClick(CurPageID: Integer): Boolean;
begin
  Result := True;
  // 鎷︽埅鈥滈€夋嫨缁勪欢鈥濋〉闈㈢殑涓嬩竴姝ュ姩浣?
  if CurPageID = wpSelectComponents then
  begin
    if (not WizardIsComponentSelected('gui')) and (not WizardIsComponentSelected('cli')) then
    begin
      MsgBox('蹇呴』鑷冲皯瀹夎涓€涓粍浠讹紙鍥惧舰鐣岄潰鐗堟垨鍛戒护琛岀増锛夛紒', mbError, MB_OK);
      Result := False;
    end;
  end;
end;

procedure CurStepChanged(CurStep: TSetupStep);
var
  OldPath: string;
  NewPath: string;
begin
  if CurStep = ssPostInstall then
  begin
    if WizardIsComponentSelected('cli') then
    begin
      if RegQueryStringValue(HKLM, 'SYSTEM\CurrentControlSet\Control\Session Manager\Environment', 'Path', OldPath) then
      begin
        if Pos(ExpandConstant('{app}'), OldPath) = 0 then
        begin
          NewPath := OldPath + ';' + ExpandConstant('{app}');
          RegWriteExpandStringValue(HKLM, 'SYSTEM\CurrentControlSet\Control\Session Manager\Environment', 'Path', NewPath);
        end;
      end;
    end;
  end;
end;
