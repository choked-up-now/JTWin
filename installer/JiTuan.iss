#define MyAppName "汲湍 (JiTuan)"
#define MyAppVersion "0.2.5"
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
Name: "desktopicon"; Description: "创建桌面快捷方式"; GroupDescription: "附加图标:"

[Files]
Source: "JiTuan_Release\jituan-gui.exe"; DestDir: "{app}"; Components: gui
Source: "JiTuan_Release\jituan-cli.exe"; DestDir: "{app}"; Components: cli
Source: "JiTuan_Release\LICENSE"; DestDir: "{app}"; Components: gui or cli
Source: "JiTuan_Release\NOTICE"; DestDir: "{app}"; Components: gui or cli

[Components]
Name: "gui"; Description: "汲湍 图形界面版 (GUI)"; Types: full compact custom
Name: "cli"; Description: "汲湍 命令行版 (CLI, 将自动加入系统 PATH)"; Types: full custom

[Icons]
Name: "{group}\汲湍"; Filename: "{app}\jituan-gui.exe"; Components: gui
Name: "{group}\卸载 汲湍"; Filename: "{uninstallexe}"
Name: "{autodesktop}\汲湍"; Filename: "{app}\jituan-gui.exe"; Tasks: desktopicon; Components: gui

[Run]
Filename: "{app}\jituan-gui.exe"; Description: "立即运行 汲湍"; Flags: nowait postinstall skipifsilent; Components: gui

[Code]
function NextButtonClick(CurPageID: Integer): Boolean;
begin
  Result := True;
  if CurPageID = wpSelectComponents then
  begin
    if (not WizardIsComponentSelected('gui')) and (not WizardIsComponentSelected('cli')) then
    begin
      MsgBox('必须至少安装一个组件（图形界面版或命令行版）！', mbError, MB_OK);
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