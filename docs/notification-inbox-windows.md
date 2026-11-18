# Windows notification capture

Open **Notification Inbox** with the launcher query **inbox**. Timers, Calendar reminders, Macro Notify steps, and other launcher events use the local Inbox. Windows notification capture is an optional addition.

The Windows listener uses a signed package with external location on Windows 10 version 2004 (build 19041) or later. The package gives the copied launcher the **MultiLauncher.NotificationInbox** identity and the Windows **userNotificationListener** capability. Windows checks the running process identity and asks for permission separately. [Microsoft documentation for packages with external location](https://learn.microsoft.com/windows/apps/desktop/modernize/grant-identity-to-nonpackaged-apps) · [Windows notification listener](https://learn.microsoft.com/windows/apps/develop/notifications/app-notifications/notification-listener)

## Build and sign the optional package

Build the launcher in the ordinary way. The optional packaging step needs the Windows SDK **MakeAppx**, **Mt**, and **SignTool** tools and a code-signing certificate trusted for Windows application packages on the destination PC. Run this from the repository root in PowerShell using the installed SDK path and the certificate's full subject.

```powershell
cargo build --release --bin multi_launcher
$sdkBin = 'C:\Program Files (x86)\Windows Kits\10\bin\10.0.26100.0\x64'
$publisher = 'CN=Your package signing certificate subject'
$bundle = Join-Path $PWD 'target\notification-inbox-package'
.\tools\notification-inbox-windows\Build-Package.ps1 -Launcher .\target\release\multi_launcher.exe -OutputDirectory $bundle -SdkBin $sdkBin -PublisherSubject $publisher -PackageVersion 1.0.0.0
```

The script writes a new output directory. It embeds the package identity in a copy under **External** and builds the unsigned sparse MSIX. It preserves the input executable, an existing readable Win32 manifest, and the original launcher profile. Editing the executable manifest changes the copied executable bytes. If you separately Authenticode sign executables for distribution, sign the copy after embedding the identity.

Copy the unsigned package to a distribution filename and sign the copy with the matching certificate. Use the signing mechanism supplied by your certificate provider. For a certificate in the current user's Personal store, specify its exact thumbprint.

```powershell
$package = Join-Path $bundle 'MultiLauncher.NotificationInbox.msix'
Copy-Item (Join-Path $bundle 'MultiLauncher.NotificationInbox.unsigned.msix') $package
& "$sdkBin\signtool.exe" sign /fd SHA256 /sha1 '<certificate thumbprint>' /s My $package
& "$sdkBin\signtool.exe" verify /pa $package
```

Production distribution requires a signature trusted by the destination PC and any required organizational deployment authorization. A self-signed certificate is suitable only for an explicitly authorized local development fixture. Importing certificates or changing deployment policy is outside the build script. [Microsoft package signing guidance](https://learn.microsoft.com/windows/msix/package/sign-app-package-using-signtool)

## Register and use

1. Put the **External** directory in its intended permanent location. Close the launcher instance for the profile you plan to use.
2. In Windows PowerShell, register the signed package with its matching absolute external directory.

   ```powershell
   $external = (Resolve-Path (Join-Path $bundle 'External')).Path
   Add-AppxPackage -Path $package -ExternalLocation $external
   ```

3. Start the copied **multi_launcher.exe** with the same working directory you use for the original launcher. For an existing shortcut, update **Target** to the copied executable and keep **Start in** pointing to the existing working directory. The working directory owns the existing settings, history, and single-instance guard.
4. Open **inbox**. In **Inbox settings**, enable **Capture Windows notifications**, apply settings, and select **Request Windows access** in the focused Inbox. Choose whether to allow access in the Windows prompt.
5. Enable **Keep notification title and body in this session** if you want text. To keep external records across sessions, separately enable **Keep Windows records across restarts**. **Clear local history** removes Inbox entries. Windows retains control of the system notification center.

**Off** means Inbox capture is disabled. **Setup required** means the running process lacks the expected identity. **Permission not granted** or **Denied** reflects Windows permission. Review the Notifications privacy page in Windows Settings to grant or revoke access. If Windows reports a setup problem after you moved the executable, re-register the package for the new external directory and relaunch the copied executable using the same profile working directory.

## Coverage and actions

The Inbox shows the current toasts Windows exposes and changes it observes while the listener is active. The capture status shows when collection started and reports limited snapshots. An incomplete snapshot leaves existing entries intact. Windows permission is checked before reading and again before the Inbox adopts results. Disabling capture or revoking access stops collection and preserves existing entries until local retention or an explicit clear removes them.

**Open source app** uses a supported installed package identity checked through Windows. **Dismiss from Windows** checks access and the selected toast's source, ID and creation time immediately before requesting removal. When text is enabled, it also compares the current title and body. Windows accepts a numeric ID for the final removal call and provides no atomic identity-and-version check. The Inbox reports the request separately from observed removal. Local read, pin, dismissal and clear only affect Multi Launcher.

## Remove the optional integration

Disable **Capture Windows notifications** and apply settings. If you also want to remove existing external Inbox records, use the explicit local clear action. Close the launcher. Find the exact package for the installed publisher and remove that specific package for the current Windows user.

```powershell
Get-AppxPackage -Name MultiLauncher.NotificationInbox |
  Select-Object PackageFullName, Publisher
Remove-AppxPackage -Package '<exact PackageFullName from above>'
```

Point your shortcut back to the ordinary executable using the same **Start in** directory. The profile and internal Inbox continue to use that directory, including any local history you chose to retain. For an authorized local test, remove only the test certificate you explicitly imported, using its exact thumbprint.
