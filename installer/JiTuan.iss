#define MyAppName "汲湍 (JiTuan)"
#define MyAppVersion "0.2.0"
#define MyAppPublisher "choked-up-now"
#define MyAppURL "https://atomgit.com/choked-up-now/JiTuan"

[Setup]
AppId={{D3C9F88A-7F1B-4C1A-9B2A-8A2C5F6D7E8B}
AppName={#MyAppName}
AppVersion={#MyAppVersion}
AppPublisher={#MyAppPublisher}
AppPublisherURL={#MyAppURL}
AppSupportURL={#MyAppURL}
; 默认安装到 Program Files
DefaultDirName={autopf}\JiTuan
DefaultGroupName={#MyAppName}
AllowNoIcons=yes
; 输出到 installers/Output 目录下
OutputDir=Output
OutputBaseFilename=JiTuan_v{#MyAppVersion}_Setup
Compression=lzma
SolidCompression=yes
WizardStyle=modern
; 要求管理员权限，因为要修改环境变量
PrivilegesRequired=admin
; 允许安装程序关闭正在运行的程序（用于自动更新）
CloseApplications=yes
RestartApplications=no

[Languages]
; 使用官方简体中文语言包
Name: "chinesesimplified"; MessagesFile: "compiler:Languages\ChineseSimplified.isl"

[Tasks]
Name: "desktopicon"; Description: "创建桌面快捷方式"; GroupDescription: "附加图标:"

[Files]
; 注意：这里的路径是相对于 JiTuan.iss 所在目录的
Source: "JiTuan_Release\jituan-gui.exe"; DestDir: "{app}"; Components: gui
Source: "JiTuan_Release\jituan-cli.exe"; DestDir: "{app}"; Components: cli
Source: "JiTuan_Release\LICENSE"; DestDir: "{app}"; Components: gui or cli
Source: "JiTuan_Release\NOTICE"; DestDir: "{app}"; Components: gui or cli

[Components]
Name: "gui"; Description: "汲湍 图形界面版 (GUI)"; Types: full compact custom; Flags: fixed
Name: "cli"; Description: "汲湍 命令行版 (CLI, 将自动加入系统 PATH)"; Types: full

[Icons]
Name: "{group}\汲湍"; Filename: "{app}\jituan-gui.exe"; Components: gui
Name: "{group}\卸载 汲湍"; Filename: "{uninstallexe}"
Name: "{autodesktop}\汲湍"; Filename: "{app}\jituan-gui.exe"; Tasks: desktopicon; Components: gui

[Run]
Filename: "{app}\jituan-gui.exe"; Description: "立即运行 汲湍"; Flags: nowait postinstall skipifsilent; Components: gui

[Code]
// 核心逻辑：如果用户勾选了 CLI，则将其所在目录加入系统环境变量 PATH
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