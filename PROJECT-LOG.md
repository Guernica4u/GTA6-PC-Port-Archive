# Project log

## 2026-11-27

Asked:
Recover when Victory starts with the app-local Direct3D 12 runtime missing.

Changed:
Pass D3D12 loader error `0x887E0003` from the isolated shader-model probe to startup. An invalid SDK with a missing DLL now triggers the existing verified download into `D3D12` beside the executable, including on Windows 11. Other unknown probe results retain their previous behavior.

Notes:
The executable requests Agility SDK version 614. On Windows 11 23H2 build 22631.6199, a missing app-local DLL caused this error before the probe could report a shader model. The matching DLL from Microsoft.Direct3D.D3D12 1.614.1 loaded successfully and the probe reported shader model 6.8.

Asked:
Make recovery automatic when the app-local runtime is removed.

Changed:
On `0x887E0003` with a missing DLL, startup uses the existing signed and hash-checked download without prompting, then reruns the probe. A working runtime below shader model 6.6 keeps its existing confirmation behavior. The startup tests cover the invalid SDK, a valid shader model, and an unknown probe result.

Notes:
The downloader explicitly imports PowerShell's built-in Utility and Security modules. The startup-launched download could otherwise fail to resolve the hash and signature cmdlets. A clean installation test verified the NuGet download and the follow-up probe from a different working directory.

Asked:
Resolve the D3D12 installer branch conflicts with main.

Changed:
Keep main's Windows runtime fallback when the local DLL is absent and its version detection for an existing DLL. If the probe still reports 0x887E0003 with a missing DLL, keep the automatic download. Refresh the launcher's selected SDK version before probing again after any successful download.

Notes:
The merged build still received 0x887E0003 without the local DLL on the Windows 11 23H2 test machine. Automatic recovery downloaded the signed SDK into the executable's D3D12 directory and the next probe reported shader model 6.8.
