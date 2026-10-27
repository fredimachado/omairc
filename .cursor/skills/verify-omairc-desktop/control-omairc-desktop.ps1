#Requires -Version 5.1
<#
.SYNOPSIS
  Isolated Windows driver for the Rust GPUI Omairc desktop client.

.DESCRIPTION
  Mirrors .cursor/skills/verify-omairc-desktop/control-omairc-desktop (the Linux
  sh driver) function-for-function. Run with pwsh (PowerShell 7) or Windows
  PowerShell 5.1. Hand-parses --flag value from $args (no param binding).

  Spike notes (Windows, DPI 144 / 150%):
  - Seeded off-screen geometry is clamped on-screen briefly; PlaceOffscreen
    after map is required (expect a short flash).
  - Force client size to 1180x760 logical after place; clamped restore can
    leave a wrong physical size.
  - CLIENT_Y_OFFSET = 0: after that resize, conversation/member tables match
    the sh driver's logical 1180x760 client coords (OS title bar is outside
    PrintWindow PW_CLIENTONLY; no extra client title strip offset).
  - WS_EX_TOOLWINDOW + HideFromTaskbar keeps PrintWindow painting (stddev
    unchanged in spike); kept for hidden launches.
  - ctrl+grave / ctrl+shift+slash resolve via VkKeyScanExW on the target
    thread layout (US: VK 0xC0 / 0xBF).
#>

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$Script:SCRIPT_DIR = Split-Path -Parent $MyInvocation.MyCommand.Path
$Script:ROOT = (Resolve-Path (Join-Path $Script:SCRIPT_DIR '..\..\..')).Path
$Script:SELF = $MyInvocation.MyCommand.Path

$defaultStateRoot = Join-Path $env:TEMP ("omairc-verify-desktop-" + $env:USERNAME)
if ($env:OMAIRC_VERIFY_DESKTOP_STATE) {
    $Script:STATE_FILE = $env:OMAIRC_VERIFY_DESKTOP_STATE
} else {
    $Script:STATE_FILE = Join-Path $defaultStateRoot 'state.json'
}
$Script:STATE_ROOT = Split-Path -Parent $Script:STATE_FILE
if (-not $Script:STATE_ROOT) { $Script:STATE_ROOT = $defaultStateRoot }

if ($env:OMAIRC_VERIFY_DESKTOP_EVIDENCE) {
    $Script:EVIDENCE_DIR = $env:OMAIRC_VERIFY_DESKTOP_EVIDENCE
} else {
    $Script:EVIDENCE_DIR = Join-Path $Script:ROOT 'test-artifacts\verify-desktop'
}

$Script:DESKTOP_PROFILE = if ($env:OMAIRC_DESKTOP_PROFILE) { $env:OMAIRC_DESKTOP_PROFILE } else { 'debug' }
if ($Script:DESKTOP_PROFILE -ne 'debug' -and $Script:DESKTOP_PROFILE -ne 'release') {
    Write-Error "control-omairc-desktop: OMAIRC_DESKTOP_PROFILE must be debug or release (got $($Script:DESKTOP_PROFILE))"
    exit 1
}
$Script:DESKTOP_FEATURES = if ($env:OMAIRC_DESKTOP_FEATURES) { $env:OMAIRC_DESKTOP_FEATURES } else { '' }

# Jump, nick-jump, and composer focus need a pause after the chord before type.
$Script:JUMP_FIELD_DELAY = 0.4
# import can return a flat frame before GPUI has painted. Drop destination
# before each grab and retry. Near-solid = tiny stddev or few colors.
$Script:SCREENSHOT_GRAB_ATTEMPTS = 5
$Script:SCREENSHOT_GRAB_DELAY = 0.3
$Script:SCREENSHOT_FLAT_STDDEV = 0.005
$Script:SCREENSHOT_FLAT_COLORS = 16

# Logical client size recipes assume (same as sh / Qt default).
$Script:LOGICAL_WIDTH = 1180
$Script:LOGICAL_HEIGHT = 760
# Measured: with client forced to 1180x760 logical, sh tables hit without offset.
$Script:CLIENT_Y_OFFSET = 0

# Loaded state fields (set by Load-State)
$Script:ROOT_STATE = $null
$Script:STATE_DIR = $null
$Script:APP_PID = $null
$Script:HWND = [IntPtr]::Zero
$Script:WINDOW = $null
$Script:BINARY = $null
$Script:DEMO = $null
$Script:VISIBLE = $null
$Script:MEMBERS_OPEN = 1
$Script:LOCALAPPDATA_OVERRIDE = $null

# ---------------------------------------------------------------------------
# Win32 helper (inline C#, compiled once, cached under STATE_ROOT parent)
# ---------------------------------------------------------------------------

$Script:OmaircDriverSource = @'
using System;
using System.Collections.Generic;
using System.IO;
using System.Runtime.InteropServices;
using System.Text;
using System.Threading;

public static class OmaircDriver {
  public const int SW_HIDE = 0;
  public const int SW_SHOWNOACTIVATE = 4;
  public const uint SWP_NOSIZE = 0x0001;
  public const uint SWP_NOMOVE = 0x0002;
  public const uint SWP_NOZORDER = 0x0004;
  public const uint SWP_NOACTIVATE = 0x0010;
  public const int GWL_EXSTYLE = -20;
  public const int WS_EX_TOOLWINDOW = 0x00000080;
  public const int WS_EX_APPWINDOW = 0x00040000;
  public const uint PW_CLIENTONLY = 0x1;
  public const uint PW_RENDERFULLCONTENT = 0x2;
  public const int BI_RGB = 0;
  public const uint SRCCOPY = 0x00CC0020;
  public const int DIB_RGB_COLORS = 0;
  public const int SM_XVIRTUALSCREEN = 76;
  public const int SM_YVIRTUALSCREEN = 77;
  public const int SM_CXVIRTUALSCREEN = 78;
  public const int SM_CYVIRTUALSCREEN = 79;
  public const int SM_CXSCREEN = 0;
  public const int SM_CYSCREEN = 1;
  public const uint STARTF_USESHOWWINDOW = 0x00000001;
  public const uint STARTF_USESTDHANDLES = 0x00000100;
  public const uint CREATE_UNICODE_ENVIRONMENT = 0x00000400;
  public const uint CREATE_NO_WINDOW = 0x08000000;
  public const int GENERIC_WRITE = unchecked((int)0x40000000);
  public const int FILE_SHARE_READ = 0x00000001;
  public const int CREATE_ALWAYS = 2;
  public const int FILE_ATTRIBUTE_NORMAL = 0x80;

  public const int WM_MOUSEMOVE = 0x0200;
  public const int WM_LBUTTONDOWN = 0x0201;
  public const int WM_LBUTTONUP = 0x0202;
  public const int MK_LBUTTON = 0x0001;
  public const int WM_KEYDOWN = 0x0100;
  public const int WM_KEYUP = 0x0101;
  public const int WM_SYSKEYDOWN = 0x0104;
  public const int WM_SYSKEYUP = 0x0105;
  public const int WM_CHAR = 0x0102;

  public const byte VK_CONTROL = 0x11;
  public const byte VK_LCONTROL = 0xA2;
  public const byte VK_SHIFT = 0x10;
  public const byte VK_LSHIFT = 0xA0;
  public const byte VK_MENU = 0x12;
  public const byte VK_LMENU = 0xA4;

  public static readonly IntPtr DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2 = new IntPtr(-4);

  [DllImport("user32.dll")] public static extern bool SetProcessDpiAwarenessContext(IntPtr value);
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hWnd);
  [DllImport("user32.dll")] public static extern bool IsWindow(IntPtr hWnd);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr hWnd);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hWnd, out RECT lpRect);
  [DllImport("user32.dll")] public static extern bool GetClientRect(IntPtr hWnd, out RECT lpRect);
  [DllImport("user32.dll")] public static extern bool ClientToScreen(IntPtr hWnd, ref POINT lpPoint);
  [DllImport("user32.dll", CharSet = CharSet.Unicode)]
  public static extern int GetWindowText(IntPtr hWnd, StringBuilder lpString, int nMaxCount);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint lpdwProcessId);
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumWindowsProc lpEnumFunc, IntPtr lParam);
  public delegate bool EnumWindowsProc(IntPtr hWnd, IntPtr lParam);
  [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr hWnd, IntPtr hWndInsertAfter, int X, int Y, int cx, int cy, uint uFlags);
  [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr hWnd, int nCmdShow);
  [DllImport("user32.dll")] public static extern int GetWindowLong(IntPtr hWnd, int nIndex);
  [DllImport("user32.dll")] public static extern int SetWindowLong(IntPtr hWnd, int nIndex, int dwNewLong);
  [DllImport("user32.dll")] public static extern uint GetDpiForWindow(IntPtr hwnd);
  [DllImport("user32.dll")] public static extern IntPtr GetKeyboardLayout(uint idThread);
  [DllImport("user32.dll", CharSet = CharSet.Unicode)]
  public static extern short VkKeyScanExW(char ch, IntPtr dwhkl);
  [DllImport("user32.dll")] public static extern bool AttachThreadInput(uint idAttach, uint idAttachTo, bool fAttach);
  [DllImport("user32.dll")] public static extern bool SetKeyboardState(byte[] lpKeyState);
  [DllImport("user32.dll")] public static extern bool GetKeyboardState(byte[] lpKeyState);
  [DllImport("user32.dll")] public static extern uint MapVirtualKey(uint uCode, uint uMapType);
  [DllImport("user32.dll")] public static extern int GetSystemMetrics(int nIndex);
  [DllImport("user32.dll")] public static extern bool SystemParametersInfo(uint uiAction, uint uiParam, ref RECT pvParam, uint fWinIni);
  [DllImport("user32.dll")] public static extern IntPtr SendMessage(IntPtr hWnd, int Msg, IntPtr wParam, IntPtr lParam);
  [DllImport("user32.dll")] public static extern IntPtr GetDC(IntPtr hWnd);
  [DllImport("user32.dll")] public static extern int ReleaseDC(IntPtr hWnd, IntPtr hDC);
  [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr hwnd, IntPtr hdcBlt, uint nFlags);
  [DllImport("kernel32.dll")] public static extern uint GetCurrentThreadId();
  [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
  public static extern bool CreateProcessW(
    string lpApplicationName, StringBuilder lpCommandLine, IntPtr lpProcessAttributes, IntPtr lpThreadAttributes,
    bool bInheritHandles, uint dwCreationFlags, IntPtr lpEnvironment, string lpCurrentDirectory,
    ref STARTUPINFO lpStartupInfo, out PROCESS_INFORMATION lpProcessInformation);
  [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
  public static extern IntPtr CreateFileW(string lpFileName, int dwDesiredAccess, int dwShareMode,
    IntPtr lpSecurityAttributes, int dwCreationDisposition, int dwFlagsAndAttributes, IntPtr hTemplateFile);
  [DllImport("kernel32.dll")] public static extern bool CloseHandle(IntPtr hObject);
  [DllImport("kernel32.dll", SetLastError = true)]
  public static extern bool SetHandleInformation(IntPtr hObject, uint dwMask, uint dwFlags);
  [DllImport("gdi32.dll")] public static extern IntPtr CreateCompatibleDC(IntPtr hdc);
  [DllImport("gdi32.dll")] public static extern IntPtr CreateDIBSection(IntPtr hdc, ref BITMAPINFO pbmi, uint iUsage, out IntPtr ppvBits, IntPtr hSection, uint dwOffset);
  [DllImport("gdi32.dll")] public static extern IntPtr SelectObject(IntPtr hdc, IntPtr hgdiobj);
  [DllImport("gdi32.dll")] public static extern bool DeleteObject(IntPtr hObject);
  [DllImport("gdi32.dll")] public static extern bool DeleteDC(IntPtr hdc);
  [DllImport("gdi32.dll")] public static extern bool BitBlt(IntPtr hdc, int x, int y, int cx, int cy, IntPtr hdcSrc, int x1, int y1, uint rop);

  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int Left, Top, Right, Bottom; }
  [StructLayout(LayoutKind.Sequential)] public struct POINT { public int X, Y; }
  [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
  public struct STARTUPINFO {
    public int cb; public string lpReserved; public string lpDesktop; public string lpTitle;
    public int dwX, dwY, dwXSize, dwYSize, dwXCountChars, dwYCountChars, dwFillAttribute;
    public int dwFlags;
    public short wShowWindow, cbReserved2; public IntPtr lpReserved2, hStdInput, hStdOutput, hStdError;
  }
  [StructLayout(LayoutKind.Sequential)]
  public struct PROCESS_INFORMATION {
    public IntPtr hProcess, hThread; public uint dwProcessId, dwThreadId;
  }
  [StructLayout(LayoutKind.Sequential)] public struct BITMAPINFOHEADER {
    public uint biSize; public int biWidth, biHeight; public ushort biPlanes, biBitCount; public uint biCompression;
    public uint biSizeImage; public int biXPelsPerMeter, biYPelsPerMeter; public uint biClrUsed, biClrImportant;
  }
  [StructLayout(LayoutKind.Sequential)] public struct BITMAPINFO { public BITMAPINFOHEADER bmiHeader; public uint bmiColors; }
  [StructLayout(LayoutKind.Sequential)] public struct SECURITY_ATTRIBUTES {
    public int nLength; public IntPtr lpSecurityDescriptor; public bool bInheritHandle;
  }

  static OmaircDriver() {
    SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
  }

  public static int VirtualLeft() { return GetSystemMetrics(SM_XVIRTUALSCREEN); }
  public static int VirtualTop() { return GetSystemMetrics(SM_YVIRTUALSCREEN); }
  public static int VirtualWidth() { return GetSystemMetrics(SM_CXVIRTUALSCREEN); }
  public static int VirtualHeight() { return GetSystemMetrics(SM_CYVIRTUALSCREEN); }
  public static int VirtualRight() { return VirtualLeft() + VirtualWidth(); }

  public static void PrimaryWorkArea(out int x, out int y, out int w, out int h) {
    RECT r = new RECT();
    // SPI_GETWORKAREA = 0x0030
    if (SystemParametersInfo(0x0030, 0, ref r, 0)) {
      x = r.Left; y = r.Top; w = r.Right - r.Left; h = r.Bottom - r.Top;
    } else {
      x = 0; y = 0; w = GetSystemMetrics(SM_CXSCREEN); h = GetSystemMetrics(SM_CYSCREEN);
    }
  }

  // logPath is accepted for API symmetry with the sh driver; stdout is not
  // redirected here. Inheriting a PowerShell pipeline handle (bInheritHandles)
  // keeps the parent pwsh stuck after launch. App logs land under the isolated
  // LOCALAPPDATA State tree; the driver also seeds STATE_DIR\logs\.
  public static bool Launch(string exe, string args, string envBlock, string logPath, out uint pid) {
    pid = 0;
    var si = new STARTUPINFO();
    si.cb = Marshal.SizeOf(typeof(STARTUPINFO));
    si.dwFlags = (int)STARTF_USESHOWWINDOW;
    si.wShowWindow = (short)SW_SHOWNOACTIVATE;
    IntPtr envPtr = IntPtr.Zero;
    try {
      if (!string.IsNullOrEmpty(envBlock)) {
        envPtr = Marshal.StringToHGlobalUni(envBlock);
      }
      if (!string.IsNullOrEmpty(logPath)) {
        try { File.WriteAllText(logPath, ""); } catch { }
      }
      string cmd = "\"" + exe + "\"";
      if (!string.IsNullOrEmpty(args)) cmd = cmd + " " + args;
      // CreateProcessW may mutate the command line buffer.
      var cmdBuf = new StringBuilder(cmd, cmd.Length + 16);
      PROCESS_INFORMATION pi;
      uint flags = CREATE_UNICODE_ENVIRONMENT;
      bool ok = CreateProcessW(null, cmdBuf, IntPtr.Zero, IntPtr.Zero, false, flags, envPtr, null, ref si, out pi);
      if (!ok) return false;
      pid = pi.dwProcessId;
      CloseHandle(pi.hThread);
      CloseHandle(pi.hProcess);
      return true;
    } finally {
      if (envPtr != IntPtr.Zero) Marshal.FreeHGlobal(envPtr);
    }
  }

  public static IntPtr FindWindow(uint pid) {
    IntPtr found = IntPtr.Zero;
    EnumWindows((h, l) => {
      if (!IsWindowVisible(h)) return true;
      uint wpid; GetWindowThreadProcessId(h, out wpid);
      if (wpid != pid) return true;
      var sb = new StringBuilder(512);
      if (GetWindowText(h, sb, sb.Capacity) <= 0) return true;
      found = h;
      return false;
    }, IntPtr.Zero);
    return found;
  }

  public static string Title(IntPtr hwnd) {
    var sb = new StringBuilder(512);
    GetWindowText(hwnd, sb, sb.Capacity);
    return sb.ToString();
  }

  public static void PlaceOffscreen(IntPtr hwnd) {
    RECT wr; GetWindowRect(hwnd, out wr);
    int w = wr.Right - wr.Left, h = wr.Bottom - wr.Top;
    int x = VirtualRight() + 200;
    SetWindowPos(hwnd, IntPtr.Zero, x, 40, w, h, SWP_NOZORDER | SWP_NOACTIVATE);
  }

  public static void PlaceVisible(IntPtr hwnd, int x, int y) {
    RECT wr; GetWindowRect(hwnd, out wr);
    int w = wr.Right - wr.Left, h = wr.Bottom - wr.Top;
    SetWindowPos(hwnd, IntPtr.Zero, x, y, w, h, SWP_NOZORDER | SWP_NOACTIVATE);
  }

  public static void SetClientSizeLogical(IntPtr hwnd, int logicalW, int logicalH, int screenX, int screenY) {
    uint dpi = GetDpiForWindow(hwnd);
    if (dpi == 0) dpi = 96;
    int clientW = (int)Math.Round(logicalW * dpi / 96.0);
    int clientH = (int)Math.Round(logicalH * dpi / 96.0);
    RECT wr, cr;
    GetWindowRect(hwnd, out wr);
    GetClientRect(hwnd, out cr);
    int padW = (wr.Right - wr.Left) - (cr.Right - cr.Left);
    int padH = (wr.Bottom - wr.Top) - (cr.Bottom - cr.Top);
    SetWindowPos(hwnd, IntPtr.Zero, screenX, screenY, clientW + padW, clientH + padH, SWP_NOZORDER | SWP_NOACTIVATE);
  }

  public static void HideFromTaskbar(IntPtr hwnd) {
    int ex = GetWindowLong(hwnd, GWL_EXSTYLE);
    ex = (ex | WS_EX_TOOLWINDOW) & ~WS_EX_APPWINDOW;
    SetWindowLong(hwnd, GWL_EXSTYLE, ex);
    ShowWindow(hwnd, SW_HIDE);
    ShowWindow(hwnd, SW_SHOWNOACTIVATE);
  }

  public static bool ForegroundRestore(IntPtr prev) {
    if (prev == IntPtr.Zero || !IsWindow(prev)) return false;
    uint curTid = GetCurrentThreadId();
    uint prevPid; uint prevTid = GetWindowThreadProcessId(prev, out prevPid);
    bool attached = false;
    if (prevTid != 0 && prevTid != curTid) attached = AttachThreadInput(curTid, prevTid, true);
    bool ok = SetForegroundWindow(prev);
    if (attached) AttachThreadInput(curTid, prevTid, false);
    return ok;
  }

  // Briefly activate the app so demo seed mention drains see an active window
  // (note_title_mark skips when window_active). Pair with ForegroundRestore.
  public static bool ForegroundActivate(IntPtr hwnd) {
    if (hwnd == IntPtr.Zero || !IsWindow(hwnd)) return false;
    uint curTid = GetCurrentThreadId();
    uint pid; uint tid = GetWindowThreadProcessId(hwnd, out pid);
    bool attached = false;
    if (tid != 0 && tid != curTid) attached = AttachThreadInput(curTid, tid, true);
    bool ok = SetForegroundWindow(hwnd);
    if (attached) AttachThreadInput(curTid, tid, false);
    return ok;
  }

  static IntPtr MakeLParam(int x, int y) {
    return (IntPtr)((y << 16) | (x & 0xFFFF));
  }

  public static void Click(IntPtr hwnd, int logicalX, int logicalY) {
    uint dpi = GetDpiForWindow(hwnd);
    if (dpi == 0) dpi = 96;
    int x = (int)Math.Round(logicalX * dpi / 96.0);
    int y = (int)Math.Round(logicalY * dpi / 96.0);
    IntPtr lp = MakeLParam(x, y);
    SendMessage(hwnd, WM_MOUSEMOVE, IntPtr.Zero, lp);
    SendMessage(hwnd, WM_LBUTTONDOWN, (IntPtr)MK_LBUTTON, lp);
    SendMessage(hwnd, WM_LBUTTONUP, IntPtr.Zero, lp);
  }

  public static void Type(IntPtr hwnd, string text) {
    if (string.IsNullOrEmpty(text)) return;
    foreach (char ch in text) {
      SendMessage(hwnd, WM_CHAR, (IntPtr)ch, IntPtr.Zero);
      Thread.Sleep(18);
    }
  }

  public static void Chord(IntPtr hwnd, string[] mods, uint vk) {
    uint pid; uint tid = GetWindowThreadProcessId(hwnd, out pid);
    uint cur = GetCurrentThreadId();
    bool attached = false;
    if (tid != 0 && tid != cur) attached = AttachThreadInput(cur, tid, true);
    byte[] state = new byte[256];
    GetKeyboardState(state);
    byte[] saved = (byte[])state.Clone();
    bool alt = false;
    if (mods != null) {
      foreach (string m in mods) {
        string mod = (m ?? "").ToLowerInvariant();
        byte vkMod = 0, vkLeft = 0;
        if (mod == "ctrl" || mod == "control") { vkMod = VK_CONTROL; vkLeft = VK_LCONTROL; }
        else if (mod == "shift") { vkMod = VK_SHIFT; vkLeft = VK_LSHIFT; }
        else if (mod == "alt" || mod == "menu") { vkMod = VK_MENU; vkLeft = VK_LMENU; alt = true; }
        if (vkMod != 0) {
          state[vkMod] = (byte)(state[vkMod] | 0x80);
          state[vkLeft] = (byte)(state[vkLeft] | 0x80);
        }
      }
    }
    SetKeyboardState(state);
    uint scan = MapVirtualKey(vk, 0);
    uint lpDown = 1u | (scan << 16);
    if (alt) lpDown |= 1u << 29;
    uint lpUp = lpDown | (1u << 30) | (1u << 31);
    int downMsg = alt ? WM_SYSKEYDOWN : WM_KEYDOWN;
    int upMsg = alt ? WM_SYSKEYUP : WM_KEYUP;
    SendMessage(hwnd, downMsg, (IntPtr)vk, (IntPtr)(int)lpDown);
    SendMessage(hwnd, upMsg, (IntPtr)vk, unchecked((IntPtr)(int)lpUp));
    SetKeyboardState(saved);
    if (attached) AttachThreadInput(cur, tid, false);
  }

  public static uint ResolveVk(IntPtr hwnd, char ch, out bool needShift) {
    uint pid; uint tid = GetWindowThreadProcessId(hwnd, out pid);
    IntPtr hkl = GetKeyboardLayout(tid);
    short scanned = VkKeyScanExW(ch, hkl);
    needShift = ((scanned >> 8) & 1) != 0;
    return (uint)(scanned & 0xFF);
  }

  public static bool Capture(IntPtr hwnd, string path) {
    RECT cr; GetClientRect(hwnd, out cr);
    int w = cr.Right - cr.Left, h = cr.Bottom - cr.Top;
    if (w <= 0 || h <= 0) return false;
    IntPtr hdcWindow = GetDC(hwnd);
    IntPtr hdcMem = CreateCompatibleDC(hdcWindow);
    var bmi = new BITMAPINFO();
    bmi.bmiHeader.biSize = (uint)Marshal.SizeOf(typeof(BITMAPINFOHEADER));
    bmi.bmiHeader.biWidth = w;
    bmi.bmiHeader.biHeight = -h;
    bmi.bmiHeader.biPlanes = 1;
    bmi.bmiHeader.biBitCount = 32;
    bmi.bmiHeader.biCompression = BI_RGB;
    IntPtr bits;
    IntPtr hbm = CreateDIBSection(hdcWindow, ref bmi, DIB_RGB_COLORS, out bits, IntPtr.Zero, 0);
    if (hbm == IntPtr.Zero) {
      DeleteDC(hdcMem);
      ReleaseDC(hwnd, hdcWindow);
      return false;
    }
    IntPtr old = SelectObject(hdcMem, hbm);
    bool ok = PrintWindow(hwnd, hdcMem, PW_CLIENTONLY | PW_RENDERFULLCONTENT);
    if (!ok) BitBlt(hdcMem, 0, 0, w, h, hdcWindow, 0, 0, SRCCOPY);
    SelectObject(hdcMem, old);
    int stride = ((w * 32 + 31) / 32) * 4;
    int imgSize = stride * h;
    byte[] pixels = new byte[imgSize];
    Marshal.Copy(bits, pixels, 0, imgSize);
    using (var fs = File.Create(path))
    using (var bw = new BinaryWriter(fs)) {
      bw.Write((ushort)0x4D42);
      bw.Write(14 + 40 + imgSize);
      bw.Write((ushort)0); bw.Write((ushort)0);
      bw.Write(14 + 40);
      bw.Write(40); bw.Write(w); bw.Write(h);
      bw.Write((ushort)1); bw.Write((ushort)32); bw.Write(0);
      bw.Write(imgSize); bw.Write(0); bw.Write(0); bw.Write(0); bw.Write(0);
      for (int y = h - 1; y >= 0; y--) bw.Write(pixels, y * stride, stride);
    }
    DeleteObject(hbm);
    DeleteDC(hdcMem);
    ReleaseDC(hwnd, hdcWindow);
    return true;
  }
}
'@

function Get-OmaircAppFolder {
    # product (default) uses omairc; --features dev uses omairc-dev.
    foreach ($part in ($Script:DESKTOP_FEATURES -split '[,\s]+')) {
        if ($part.Trim() -eq 'dev') { return 'omairc-dev' }
    }
    return 'omairc'
}

function Parse-InvariantDouble {
    param([string]$Token)
    $value = 0.0
    $ok = [double]::TryParse(
        $Token,
        [System.Globalization.NumberStyles]::Float,
        [System.Globalization.CultureInfo]::InvariantCulture,
        [ref]$value
    )
    if ($ok) { return $value }
    return $null
}

function Ensure-OmaircDriver {
    if ([type]::GetType('OmaircDriver', $false)) { return }
    if (-not (Test-Path -LiteralPath $Script:STATE_ROOT)) {
        New-Item -ItemType Directory -Force -Path $Script:STATE_ROOT | Out-Null
    }
    $sha = [System.Security.Cryptography.SHA256]::Create()
    try {
        $bytes = [System.Text.Encoding]::UTF8.GetBytes($Script:OmaircDriverSource)
        $hash = ($sha.ComputeHash($bytes) | ForEach-Object { $_.ToString('x2') }) -join ''
    } finally {
        $sha.Dispose()
    }
    # pwsh and Windows PowerShell 5.1 emit incompatible assemblies; key the
    # cache by edition + CLR version so Add-Type -Path does not load the wrong one.
    $edition = if ($PSVersionTable.ContainsKey('PSEdition') -and $PSVersionTable.PSEdition) {
        [string]$PSVersionTable.PSEdition
    } else {
        'Desktop'
    }
    $runtime = [Environment]::Version.ToString()
    $dll = Join-Path $Script:STATE_ROOT ("OmaircDesktopDriver-$hash-$edition-$runtime.dll")
    if (-not (Test-Path -LiteralPath $dll)) {
        Add-Type -TypeDefinition $Script:OmaircDriverSource -OutputAssembly $dll -ErrorAction Stop
    }
    try {
        Add-Type -Path $dll -ErrorAction Stop
    } catch {
        Remove-Item -LiteralPath $dll -Force -ErrorAction SilentlyContinue
        Add-Type -TypeDefinition $Script:OmaircDriverSource -OutputAssembly $dll -ErrorAction Stop
        Add-Type -Path $dll -ErrorAction Stop
    }
}

function Die {
    param([Parameter(ValueFromRemainingArguments = $true)][string[]]$Message)
    $text = ($Message -join ' ')
    [Console]::Error.WriteLine("control-omairc-desktop: $text")
    exit 1
}

function Get-StateObject {
    if (-not (Test-Path -LiteralPath $Script:STATE_FILE)) { return $null }
    try {
        return (Get-Content -LiteralPath $Script:STATE_FILE -Raw -Encoding UTF8 | ConvertFrom-Json)
    } catch {
        return $null
    }
}

function Show-Usage {
    @'
control-omairc-desktop - isolated driver for the Rust GPUI Omairc desktop client (Windows)

Usage:
  control-omairc-desktop.ps1 launch [--demo-server] [--visible]
  control-omairc-desktop.ps1 doctor
  control-omairc-desktop.ps1 title
  control-omairc-desktop.ps1 wait-title --exact "<title>"
  control-omairc-desktop.ps1 focus-composer
  control-omairc-desktop.ps1 composer
  control-omairc-desktop.ps1 jump --query "<text>"
  control-omairc-desktop.ps1 walk --down|--up [--times N]
  control-omairc-desktop.ps1 unread
  control-omairc-desktop.ps1 toggle-members
  control-omairc-desktop.ps1 focus-members
  control-omairc-desktop.ps1 nick-jump --query "<text>"
  control-omairc-desktop.ps1 status
  control-omairc-desktop.ps1 connect
  control-omairc-desktop.ps1 type --text "<text>"
  control-omairc-desktop.ps1 key --key <xdotool-key>
  control-omairc-desktop.ps1 send --text "<text>"
  control-omairc-desktop.ps1 click --x N --y N
  control-omairc-desktop.ps1 click-conversation --name <#channel|nick>
  control-omairc-desktop.ps1 click-member --name <nick>
  control-omairc-desktop.ps1 click-people [--hidden]
  control-omairc-desktop.ps1 click-network
  control-omairc-desktop.ps1 click-edit
  control-omairc-desktop.ps1 click-version
  control-omairc-desktop.ps1 click-send
  control-omairc-desktop.ps1 screenshot --feature <id> --name <stem>
  control-omairc-desktop.ps1 compare --before <path> --after <path>
  control-omairc-desktop.ps1 run <feature> [--dry-run]
  control-omairc-desktop.ps1 cleanup

Default launch places the window off-screen and hides the taskbar button.
--visible centres on the primary work area without taking focus. Do not put
--visible in feature recipes. Requires ImageMagick magick and a cargo/MSVC
toolchain for launch.
'@ | Write-Output
}

function Require-Tools {
    $missing = @()
    foreach ($name in @('magick', 'cargo')) {
        if (-not (Get-Command $name -ErrorAction SilentlyContinue)) {
            $missing += $name
        }
    }
    if ($missing.Count -gt 0) {
        Die "missing desktop tools: $($missing -join ' '). Install ImageMagick (magick on PATH) and Rust/cargo with MSVC."
    }
}

function Delay-JumpField {
    Start-Sleep -Milliseconds ([int]([double]$Script:JUMP_FIELD_DELAY * 1000))
}

function Dump-OmaircLogs {
    param([string]$StateDir, [string]$LogPath)
    $appFolder = Get-OmaircAppFolder
    $candidates = @(
        $LogPath
        (Join-Path $StateDir "lad\State\$appFolder\omairc.log")
        (Join-Path $StateDir "lad\$appFolder\omairc.log")
    )
    foreach ($path in $candidates) {
        if ($path -and (Test-Path -LiteralPath $path) -and (Get-Item -LiteralPath $path).Length -gt 0) {
            [Console]::Error.WriteLine("control-omairc-desktop: omairc.log ($path):")
            Get-Content -LiteralPath $path -ErrorAction SilentlyContinue | ForEach-Object { [Console]::Error.WriteLine($_) }
            return
        }
    }
}

function Delay-KeySettle {
    Start-Sleep -Milliseconds 200
}

function Get-ConversationY {
    param([string]$Name)
    # Logical 1180x760 client coords (CLIENT_Y_OFFSET applied at click time).
    switch ($Name) {
        '#desktop' { return 124 }
        '#help' { return 160 }
        '#omarchy' { return 196 }
        '#ricing' { return 232 }
        'anna' { return 276 }
        'dax' { return 312 }
        default { Die "unknown conversation '$Name'. Use #desktop, #help, #omarchy, #ricing, anna, or dax." }
    }
}

function Get-MemberY {
    param([string]$Name)
    switch ($Name) {
        'fred' { return 71 }
        'anna' { return 114 }
        'dax' { return 157 }
        'mira' { return 200 }
        'kai' { return 243 }
        'teo' { return 286 }
        'ivy' { return 329 }
        'lena' { return 372 }
        'max' { return 415 }
        'nora' { return 458 }
        'sam' { return 501 }
        'sol' { return 544 }
        default { Die "unknown member '$Name'." }
    }
}

function Load-State {
    if (-not (Test-Path -LiteralPath $Script:STATE_FILE)) {
        Die "no active instance. Run: $($Script:SELF) launch [--demo-server]"
    }
    $obj = Get-Content -LiteralPath $Script:STATE_FILE -Raw -Encoding UTF8 | ConvertFrom-Json
    $Script:ROOT_STATE = [string]$obj.ROOT
    $Script:STATE_DIR = [string]$obj.STATE_DIR
    if ($obj.EVIDENCE_DIR) { $Script:EVIDENCE_DIR = [string]$obj.EVIDENCE_DIR }
    $Script:APP_PID = [int]$obj.APP_PID
    if ($obj.HWND) {
        $Script:HWND = [IntPtr]([int64]$obj.HWND)
        $Script:WINDOW = ('0x{0:X}' -f ([int64]$obj.HWND))
    } elseif ($obj.WINDOW) {
        $Script:WINDOW = [string]$obj.WINDOW
        $hex = $Script:WINDOW
        if ($hex -match '^0x') { $Script:HWND = [IntPtr][Convert]::ToInt64($hex, 16) }
        else { $Script:HWND = [IntPtr][int64]$hex }
    } else {
        Die 'state file is missing HWND/WINDOW'
    }
    $Script:BINARY = [string]$obj.BINARY
    $Script:DEMO = $obj.DEMO
    $Script:VISIBLE = $obj.VISIBLE
    if ($null -ne $obj.MEMBERS_OPEN) { $Script:MEMBERS_OPEN = [int]$obj.MEMBERS_OPEN } else { $Script:MEMBERS_OPEN = 1 }
    if ($obj.LOCALAPPDATA) { $Script:LOCALAPPDATA_OVERRIDE = [string]$obj.LOCALAPPDATA }
    elseif ($Script:STATE_DIR) { $Script:LOCALAPPDATA_OVERRIDE = Join-Path $Script:STATE_DIR 'lad' }
    if (-not $Script:APP_PID) { Die 'state file is missing APP_PID' }
    Ensure-OmaircDriver
}

function Write-State {
    if (-not (Test-Path -LiteralPath (Split-Path -Parent $Script:STATE_FILE))) {
        New-Item -ItemType Directory -Force -Path (Split-Path -Parent $Script:STATE_FILE) | Out-Null
    }
    $hwndVal = [int64]$Script:HWND
    $obj = [ordered]@{
        ROOT        = $Script:ROOT
        STATE_DIR   = $Script:STATE_DIR
        EVIDENCE_DIR = $Script:EVIDENCE_DIR
        APP_PID     = $Script:APP_PID
        HWND        = $hwndVal
        WINDOW      = ('0x{0:X}' -f $hwndVal)
        BINARY      = $Script:BINARY
        DEMO        = $Script:DEMO
        VISIBLE     = $Script:VISIBLE
        MEMBERS_OPEN = $Script:MEMBERS_OPEN
        LOCALAPPDATA = $Script:LOCALAPPDATA_OVERRIDE
    }
    ($obj | ConvertTo-Json -Depth 5) | Set-Content -LiteralPath $Script:STATE_FILE -Encoding UTF8
}

function Test-ProcessAlive {
    param([int]$ProcessId)
    if ($ProcessId -le 0) { return $false }
    try {
        $p = Get-Process -Id $ProcessId -ErrorAction Stop
        return -not $p.HasExited
    } catch {
        return $false
    }
}

function Stop-AppPid {
    param([int]$ProcessId)
    if ($ProcessId -le 0) { return }
    if (Test-ProcessAlive -ProcessId $ProcessId) {
        Stop-Process -Id $ProcessId -Force -ErrorAction SilentlyContinue
        Start-Sleep -Milliseconds 200
    }
}

function Get-WindowTitle {
    Ensure-OmaircDriver
    return [OmaircDriver]::Title($Script:HWND)
}

function Invoke-ClickXy {
    param([int]$X, [int]$Y)
    Ensure-OmaircDriver
    $y = $Y + [int]$Script:CLIENT_Y_OFFSET
    [OmaircDriver]::Click($Script:HWND, $X, $y)
    Start-Sleep -Milliseconds 200
}

function Build-EnvBlock {
    param([hashtable]$Overrides)
    $pairs = New-Object System.Collections.Generic.List[string]
    foreach ($entry in [System.Environment]::GetEnvironmentVariables().GetEnumerator()) {
        $key = [string]$entry.Key
        if ($Overrides.ContainsKey($key)) { continue }
        $pairs.Add($key + '=' + [string]$entry.Value)
    }
    foreach ($key in $Overrides.Keys) {
        $pairs.Add($key + '=' + [string]$Overrides[$key])
    }
    return (($pairs -join "`0") + "`0`0")
}

function Test-DoctorHealthy {
    Ensure-OmaircDriver
    $obj = Get-StateObject
    if (-not $obj) { return $false }
    try {
        $appPid = [int]$obj.APP_PID
        if (-not (Test-ProcessAlive -ProcessId $appPid)) { return $false }
        $hwnd = [IntPtr]::Zero
        if ($obj.HWND) { $hwnd = [IntPtr]([int64]$obj.HWND) }
        elseif ($obj.WINDOW -and ([string]$obj.WINDOW -match '^0x')) {
            $hwnd = [IntPtr][Convert]::ToInt64([string]$obj.WINDOW, 16)
        } else { return $false }
        if (-not [OmaircDriver]::IsWindow($hwnd)) { return $false }
        $found = [OmaircDriver]::FindWindow([uint32]$appPid)
        if ($found -ne $hwnd) { return $false }
        $title = [OmaircDriver]::Title($hwnd)
        if ($title -notmatch ' - Omairc$' -and $title -notmatch ' Status$' -and $title -ne 'Status') { return $false }
        $binary = [string]$obj.BINARY
        if (-not (Test-Path -LiteralPath $binary)) { return $false }
        $norm = $binary -replace '/', '\'
        if ($norm -notmatch '\\client\\target\\(debug|release)\\omairc\.exe$') { return $false }
        $lad = $null
        if ($obj.LOCALAPPDATA) { $lad = [string]$obj.LOCALAPPDATA }
        elseif ($obj.STATE_DIR) { $lad = Join-Path ([string]$obj.STATE_DIR) 'lad' }
        if (-not $lad -or -not (Test-Path -LiteralPath $lad)) { return $false }
        $appFolder = Get-OmaircAppFolder
        if (-not (Test-Path -LiteralPath (Join-Path $lad $appFolder))) { return $false }
        return $true
    } catch {
        return $false
    }
}

function Get-DoctorDemoFlag {
    $obj = Get-StateObject
    if (-not $obj) { return $null }
    if (-not (Test-DoctorHealthy)) { return $null }
    if ([int]$obj.DEMO -eq 1) { return 'yes' }
    return 'no'
}

function Invoke-Launch {
    param([string[]]$LaunchArgs)
    Ensure-OmaircDriver
    Require-Tools
    $demo = $false
    $visible = $false
    $i = 0
    while ($i -lt $LaunchArgs.Count) {
        switch ($LaunchArgs[$i]) {
            '--demo-server' { $demo = $true; $i++ }
            '--visible' { $visible = $true; $i++ }
            default { Die "unknown launch argument: $($LaunchArgs[$i]). Use launch [--demo-server] [--visible]." }
        }
    }

    if (Test-Path -LiteralPath $Script:STATE_FILE) {
        if (Test-DoctorHealthy) {
            Die 'an isolated Omairc instance is already running. Run cleanup first, or doctor to inspect it.'
        }
        [Console]::Error.WriteLine('control-omairc-desktop: clearing stale run state')
        try { Invoke-Cleanup | Out-Null } catch { }
    }

    $cargoArgs = @('build')
    if ($Script:DESKTOP_PROFILE -eq 'release') { $cargoArgs += '--release' }
    if ($Script:DESKTOP_FEATURES) {
        $cargoArgs += '--features'
        $cargoArgs += $Script:DESKTOP_FEATURES
    }
    Push-Location (Join-Path $Script:ROOT 'client')
    try {
        & cargo @cargoArgs
        if ($LASTEXITCODE -ne 0) { Die "cargo build failed (exit $LASTEXITCODE)" }
    } finally {
        Pop-Location
    }

    $stateDir = Join-Path $env:TEMP ("omairc-verify-desktop." + [guid]::NewGuid().ToString('n').Substring(0, 8))
    New-Item -ItemType Directory -Force -Path $stateDir | Out-Null
    $binary = Join-Path $Script:ROOT ("client\target\$($Script:DESKTOP_PROFILE)\omairc.exe")
    if (-not (Test-Path -LiteralPath $binary)) { Die "missing $binary after cargo build ($($Script:DESKTOP_PROFILE))" }

    $lad = Join-Path $stateDir 'lad'
    $appFolder = Get-OmaircAppFolder
    $confDir = Join-Path $lad $appFolder
    $logDir = Join-Path $stateDir 'logs'
    New-Item -ItemType Directory -Force -Path $confDir, $logDir, $Script:EVIDENCE_DIR | Out-Null

    # Seed geometry before launch. Off-screen X is often clamped by Windows;
    # PlaceOffscreen after map is authoritative (brief on-screen flash OK).
    $seedX = 80
    $seedY = 40
    if ($visible) {
        $wx = 0; $wy = 0; $ww = 0; $wh = 0
        [OmaircDriver]::PrimaryWorkArea([ref]$wx, [ref]$wy, [ref]$ww, [ref]$wh)
        $seedX = $wx + [math]::Max(0, [int](($ww - $Script:LOGICAL_WIDTH) / 2))
        $seedY = $wy + [math]::Max(0, [int](($wh - $Script:LOGICAL_HEIGHT) / 2))
    } else {
        $seedX = [OmaircDriver]::VirtualRight() + 200
        $seedY = 40
    }
    $conf = @"
[migration]
qtRegistryImported=true
[window]
geometry=@Rect($seedX $seedY $($Script:LOGICAL_WIDTH) $($Script:LOGICAL_HEIGHT))
maximized=false
"@
    Set-Content -LiteralPath (Join-Path $confDir 'omairc.conf') -Value $conf -Encoding ascii

    $overrides = @{
        LOCALAPPDATA              = $lad
        OMAIRC_ALLOW_MULTI        = '1'
        OMAIRC_SKIP_NOTIFICATIONS = '1'
    }
    $envBlock = Build-EnvBlock -Overrides $overrides
    $appArgs = ''
    if ($demo) { $appArgs = '--demo-server' }
    $logPath = Join-Path $logDir 'omairc.log'

    $prevFg = [OmaircDriver]::GetForegroundWindow()
    $appPid = [uint32]0
    $ok = [OmaircDriver]::Launch($binary, $appArgs, $envBlock, $logPath, [ref]$appPid)
    if (-not $ok -or $appPid -eq 0) {
        if (Test-Path -LiteralPath $stateDir) {
            Remove-Item -LiteralPath $stateDir -Recurse -Force -ErrorAction SilentlyContinue
        }
        Die "CreateProcessW failed for $binary"
    }

    $hwnd = [IntPtr]::Zero
    for ($attempt = 0; $attempt -lt 150; $attempt++) {
        Start-Sleep -Milliseconds 100
        if (-not (Test-ProcessAlive -ProcessId ([int]$appPid))) {
            Dump-OmaircLogs -StateDir $stateDir -LogPath $logPath
            if (Test-Path -LiteralPath $stateDir) {
                Remove-Item -LiteralPath $stateDir -Recurse -Force -ErrorAction SilentlyContinue
            }
            Die "Omairc did not open; see $logPath"
        }
        $hwnd = [OmaircDriver]::FindWindow($appPid)
        if ($hwnd -ne [IntPtr]::Zero) { break }
    }
    if ($hwnd -eq [IntPtr]::Zero) {
        Dump-OmaircLogs -StateDir $stateDir -LogPath $logPath
        Stop-AppPid -ProcessId ([int]$appPid)
        if (Test-Path -LiteralPath $stateDir) {
            Remove-Item -LiteralPath $stateDir -Recurse -Force -ErrorAction SilentlyContinue
        }
        Die "Omairc did not open; see $logPath"
    }

    # Match the sh driver's post-map windowfocus: keep the window active while
    # the demo seed drains mentions so note_title_mark does not stick an
    # attention title. Restore the caller's foreground after placement.
    [void][OmaircDriver]::ForegroundActivate($hwnd)
    Start-Sleep -Milliseconds 800

    if ($visible) {
        $wx = 0; $wy = 0; $ww = 0; $wh = 0
        [OmaircDriver]::PrimaryWorkArea([ref]$wx, [ref]$wy, [ref]$ww, [ref]$wh)
        $placeX = $wx + [math]::Max(0, [int](($ww - $Script:LOGICAL_WIDTH) / 2))
        $placeY = $wy + [math]::Max(0, [int](($wh - $Script:LOGICAL_HEIGHT) / 2))
        [OmaircDriver]::SetClientSizeLogical($hwnd, $Script:LOGICAL_WIDTH, $Script:LOGICAL_HEIGHT, $placeX, $placeY)
        $placement = 'onscreen'
    } else {
        $offX = [OmaircDriver]::VirtualRight() + 200
        [OmaircDriver]::SetClientSizeLogical($hwnd, $Script:LOGICAL_WIDTH, $Script:LOGICAL_HEIGHT, $offX, 40)
        [OmaircDriver]::HideFromTaskbar($hwnd)
        $placement = 'offscreen'
    }

    [void][OmaircDriver]::ForegroundRestore($prevFg)

    # Demo channels start with the member panel open (Shell::members_open).
    $Script:STATE_DIR = $stateDir
    $Script:APP_PID = [int]$appPid
    $Script:HWND = $hwnd
    $Script:WINDOW = ('0x{0:X}' -f ([int64]$hwnd))
    $Script:BINARY = $binary
    $Script:DEMO = $(if ($demo) { 1 } else { 0 })
    $Script:VISIBLE = $(if ($visible) { 1 } else { 0 })
    $Script:MEMBERS_OPEN = 1
    $Script:LOCALAPPDATA_OVERRIDE = $lad
    Write-State

    Write-Output 'launched isolated Omairc'
    Write-Output $(if ($demo) { 'demo=yes' } else { 'demo=no' })
    Write-Output $(if ($visible) { 'visible=yes' } else { 'visible=no' })
    Write-Output "placement=$placement"
    Write-Output "pid=$($Script:APP_PID)"
    Write-Output "window=$($Script:WINDOW)"
    Write-Output "title=$(Get-WindowTitle)"
    Write-Output "state=$($Script:STATE_FILE)"
}

function Invoke-Doctor {
    Load-State
    if (-not (Test-ProcessAlive -ProcessId $Script:APP_PID)) {
        Die "Omairc pid $($Script:APP_PID) is not running"
    }
    if (-not (Test-Path -LiteralPath $Script:BINARY)) {
        Die "binary missing: $($Script:BINARY)"
    }
    if (-not [OmaircDriver]::IsWindow($Script:HWND)) {
        Die "window $($Script:WINDOW) is gone"
    }
    $found = [OmaircDriver]::FindWindow([uint32]$Script:APP_PID)
    if ($found -ne $Script:HWND) {
        Die "window $($Script:WINDOW) is not the Omairc window for pid $($Script:APP_PID)"
    }
    $title = Get-WindowTitle
    if ($title -notmatch ' - Omairc$' -and $title -notmatch ' Status$' -and $title -ne 'Status') {
        Die "unexpected window title '$title'"
    }
    $norm = $Script:BINARY -replace '/', '\'
    if ($norm -notmatch '\\client\\target\\(debug|release)\\omairc\.exe$') {
        Die "binary is not the desktop binary: $($Script:BINARY)"
    }
    if (-not $Script:LOCALAPPDATA_OVERRIDE -or -not (Test-Path -LiteralPath $Script:LOCALAPPDATA_OVERRIDE)) {
        Die "LOCALAPPDATA override missing: $($Script:LOCALAPPDATA_OVERRIDE)"
    }
    $appFolder = Get-OmaircAppFolder
    $appConf = Join-Path $Script:LOCALAPPDATA_OVERRIDE $appFolder
    if (-not (Test-Path -LiteralPath $appConf)) {
        Die "app folder missing under LOCALAPPDATA: $appConf"
    }
    $placement = if ([int]$Script:VISIBLE -eq 1) { 'onscreen' } else { 'offscreen' }
    Write-Output 'ok isolated Omairc'
    Write-Output "binary=$($Script:BINARY)"
    Write-Output "app_pid=$($Script:APP_PID)"
    Write-Output "window=$($Script:WINDOW)"
    Write-Output "title=$title"
    Write-Output "localappdata=$($Script:LOCALAPPDATA_OVERRIDE)"
    Write-Output "evidence=$($Script:EVIDENCE_DIR)"
    Write-Output $(if ([int]$Script:DEMO -eq 1) { 'demo=yes' } else { 'demo=no' })
    Write-Output $(if ([int]$Script:VISIBLE -eq 1) { 'visible=yes' } else { 'visible=no' })
    Write-Output "placement=$placement"
}

function Invoke-Title {
    Load-State
    Write-Output (Get-WindowTitle)
}

function Invoke-WaitTitle {
    param([string[]]$WaitArgs)
    $exact = $null
    $i = 0
    while ($i -lt $WaitArgs.Count) {
        if ($WaitArgs[$i] -eq '--exact') {
            if ($i + 1 -ge $WaitArgs.Count) { Die 'wait-title requires --exact' }
            $exact = $WaitArgs[$i + 1]
            $i += 2
        } else {
            Die "unknown wait-title argument: $($WaitArgs[$i])"
        }
    }
    if (-not $exact) { Die 'wait-title requires --exact' }
    Load-State
    for ($attempt = 0; $attempt -lt 30; $attempt++) {
        $title = Get-WindowTitle
        if ($title -eq $exact) {
            Write-Output $title
            return
        }
        Start-Sleep -Milliseconds 100
    }
    Die "expected title '$exact', got '$(Get-WindowTitle)'"
}

function Reject-QtOnlyVerb {
    param([string]$Verb)
    Die "verb '$Verb' is retired (Qt offscreen QML suite). Desktop proof uses control-omairc-desktop only."
}

function Invoke-Click {
    param([string[]]$ClickArgs)
    $x = $null; $y = $null
    $i = 0
    while ($i -lt $ClickArgs.Count) {
        switch ($ClickArgs[$i]) {
            '--x' { $x = [int]$ClickArgs[$i + 1]; $i += 2 }
            '--y' { $y = [int]$ClickArgs[$i + 1]; $i += 2 }
            default { Die "unknown click argument: $($ClickArgs[$i])" }
        }
    }
    if ($null -eq $x -or $null -eq $y) { Die 'click requires --x and --y' }
    Load-State
    Invoke-ClickXy -X $x -Y $y
}

function Invoke-ClickConversation {
    param([string[]]$ArgsList)
    $name = $null
    $i = 0
    while ($i -lt $ArgsList.Count) {
        if ($ArgsList[$i] -eq '--name') {
            $name = $ArgsList[$i + 1]; $i += 2
        } else { Die "unknown click-conversation argument: $($ArgsList[$i])" }
    }
    if (-not $name) { Die 'click-conversation requires --name' }
    Load-State
    Invoke-ClickXy -X 120 -Y (Get-ConversationY -Name $name)
}

function Invoke-ClickMember {
    param([string[]]$ArgsList)
    $name = $null
    $i = 0
    while ($i -lt $ArgsList.Count) {
        if ($ArgsList[$i] -eq '--name') {
            $name = $ArgsList[$i + 1]; $i += 2
        } else { Die "unknown click-member argument: $($ArgsList[$i])" }
    }
    if (-not $name) { Die 'click-member requires --name' }
    Load-State
    Invoke-ClickXy -X 1070 -Y (Get-MemberY -Name $name)
}

function Invoke-ClickPeople {
    param([string[]]$ArgsList)
    $hidden = $false
    foreach ($a in $ArgsList) {
        if ($a -eq '--hidden') { $hidden = $true }
        else { Die "unknown click-people argument: $a. Use click-people or click-people --hidden." }
    }
    Load-State
    if ($hidden) { Invoke-ClickXy -X 1124 -Y 36 }
    else { Invoke-ClickXy -X 908 -Y 36 }
}

function Invoke-ClickNetwork { Load-State; Invoke-ClickXy -X 120 -Y 36 }
function Invoke-ClickEdit { Load-State; Invoke-ClickXy -X 220 -Y 36 }
function Invoke-ClickSend { Load-State; Invoke-ClickXy -X 909 -Y 717 }

function Invoke-ClickVersion {
    Load-State
    Invoke-ClickXy -X 214 -Y 734
    Invoke-Key -KeyArgs @('--key', 'ctrl+shift+slash')
    Delay-JumpField
}

function Resolve-KeyToken {
    param([IntPtr]$Hwnd, [string]$Key)
    # Normalize xdotool-style Control_L+/Shift_L+/Alt_L+ prefixes.
    $k = $Key
    $k = $k -replace '^Control_L\+', 'ctrl+'
    $k = $k -replace '^Shift_L\+', 'shift+'
    $k = $k -replace '^Alt_L\+', 'alt+'
    # Same rewrite table as the sh driver for ApplicationShortcuts.
    switch ($k) {
        'ctrl+slash' { $k = 'ctrl+slash' }
        'ctrl+shift+slash' { $k = 'ctrl+shift+slash' }
    }
    $mods = New-Object System.Collections.Generic.List[string]
    $rest = $k
    while ($true) {
        if ($rest -match '^(ctrl|control|shift|alt)\+(.*)$') {
            $mod = $Matches[1].ToLowerInvariant()
            if ($mod -eq 'control') { $mod = 'ctrl' }
            $mods.Add($mod)
            $rest = $Matches[2]
        } else { break }
    }
    $named = @{
        'Return' = 0x0D; 'Escape' = 0x1B; 'Tab' = 0x09; 'BackSpace' = 0x08
        'Delete' = 0x2E; 'Home' = 0x24; 'End' = 0x23
        'Page_Up' = 0x21; 'Page_Down' = 0x22
        'Left' = 0x25; 'Up' = 0x26; 'Right' = 0x27; 'Down' = 0x28
        'Space' = 0x20
    }
    for ($f = 1; $f -le 12; $f++) { $named["F$f"] = 0x70 + ($f - 1) }

    $vk = [uint32]0
    if ($named.ContainsKey($rest)) {
        $vk = [uint32]$named[$rest]
    } elseif ($rest.Length -eq 1) {
        $needShift = $false
        $vk = [OmaircDriver]::ResolveVk($Hwnd, [char]$rest[0], [ref]$needShift)
        if ($needShift -and -not ($mods -contains 'shift')) { $mods.Add('shift') }
    } elseif ($rest -eq 'slash') {
        $needShift = $false
        $vk = [OmaircDriver]::ResolveVk($Hwnd, [char]'/', [ref]$needShift)
        if ($needShift -and -not ($mods -contains 'shift')) { $mods.Add('shift') }
    } elseif ($rest -eq 'grave') {
        $needShift = $false
        $vk = [OmaircDriver]::ResolveVk($Hwnd, [char]'`', [ref]$needShift)
        if ($needShift -and -not ($mods -contains 'shift')) { $mods.Add('shift') }
    } elseif ($rest -eq 'comma') {
        $needShift = $false
        $vk = [OmaircDriver]::ResolveVk($Hwnd, [char]',', [ref]$needShift)
        if ($needShift -and -not ($mods -contains 'shift')) { $mods.Add('shift') }
    } else {
        Die "unsupported key token '$Key'"
    }
    return @{ Mods = $mods.ToArray(); Vk = $vk }
}

function Invoke-Key {
    param([string[]]$KeyArgs)
    $key = $null
    $i = 0
    while ($i -lt $KeyArgs.Count) {
        if ($KeyArgs[$i] -eq '--key') {
            $key = $KeyArgs[$i + 1]; $i += 2
        } else { Die "unknown key argument: $($KeyArgs[$i])" }
    }
    if (-not $key) { Die 'key requires --key' }
    Load-State
    $resolved = Resolve-KeyToken -Hwnd $Script:HWND -Key $key
    [OmaircDriver]::Chord($Script:HWND, $resolved.Mods, $resolved.Vk)
    Delay-KeySettle
}

function Invoke-Type {
    param([string[]]$TypeArgs)
    $text = $null
    $i = 0
    while ($i -lt $TypeArgs.Count) {
        if ($TypeArgs[$i] -eq '--text') {
            $text = $TypeArgs[$i + 1]; $i += 2
        } else { Die "unknown type argument: $($TypeArgs[$i])" }
    }
    if ($null -eq $text -or $text -eq '') { Die 'type requires --text' }
    Load-State
    [OmaircDriver]::Type($Script:HWND, $text)
}

function Invoke-FocusComposer {
    param([string[]]$ArgsList)
    if ($ArgsList -and $ArgsList.Count -gt 0) { Die 'focus-composer does not take arguments' }
    Invoke-Key -KeyArgs @('--key', 'ctrl+l')
    Delay-JumpField
}

function Invoke-Composer {
    param([string[]]$ArgsList)
    if ($ArgsList -and $ArgsList.Count -gt 0) { Die 'composer does not take arguments' }
    Invoke-Key -KeyArgs @('--key', 'ctrl+l')
    Delay-JumpField
}

function Invoke-Send {
    param([string[]]$SendArgs)
    $text = $null
    $i = 0
    while ($i -lt $SendArgs.Count) {
        if ($SendArgs[$i] -eq '--text') {
            $text = $SendArgs[$i + 1]; $i += 2
        } else { Die "unknown send argument: $($SendArgs[$i])" }
    }
    if ($null -eq $text -or $text -eq '') { Die 'send requires --text' }
    Invoke-Key -KeyArgs @('--key', 'ctrl+l')
    Delay-JumpField
    Invoke-Type -TypeArgs @('--text', $text)
    Invoke-Key -KeyArgs @('--key', 'Return')
}

function Invoke-QueryChord {
    param([string]$Verb, [string]$Chord, [string[]]$ArgsList)
    $query = $null
    $i = 0
    while ($i -lt $ArgsList.Count) {
        if ($ArgsList[$i] -eq '--query') {
            if ($i + 1 -ge $ArgsList.Count) { Die "$Verb requires --query" }
            $query = $ArgsList[$i + 1]; $i += 2
        } else { Die "unknown $Verb argument: $($ArgsList[$i])" }
    }
    if (-not $query) { Die "$Verb requires --query" }
    Invoke-Key -KeyArgs @('--key', $Chord)
    Delay-JumpField
    Invoke-Type -TypeArgs @('--text', $query)
    Invoke-Key -KeyArgs @('--key', 'Return')
}

function Invoke-Jump { param([string[]]$ArgsList); Invoke-QueryChord -Verb 'jump' -Chord 'ctrl+k' -ArgsList $ArgsList }
function Invoke-NickJump { param([string[]]$ArgsList); Invoke-QueryChord -Verb 'nick-jump' -Chord 'ctrl+shift+k' -ArgsList $ArgsList }

function Invoke-Walk {
    param([string[]]$WalkArgs)
    $direction = $null
    $times = 1
    $i = 0
    while ($i -lt $WalkArgs.Count) {
        switch ($WalkArgs[$i]) {
            '--down' {
                if ($direction) { Die 'walk accepts only one of --down or --up' }
                $direction = 'down'; $i++
            }
            '--up' {
                if ($direction) { Die 'walk accepts only one of --down or --up' }
                $direction = 'up'; $i++
            }
            '--times' {
                if ($i + 1 -ge $WalkArgs.Count) { Die 'walk requires --times N' }
                $times = [int]$WalkArgs[$i + 1]; $i += 2
            }
            default { Die "unknown walk argument: $($WalkArgs[$i])" }
        }
    }
    if (-not $direction) { Die 'walk requires --down or --up' }
    if ($times -lt 1) { Die 'walk --times requires a positive integer' }
    $key = if ($direction -eq 'down') { 'alt+Down' } else { 'alt+Up' }
    for ($step = 0; $step -lt $times; $step++) {
        Invoke-Key -KeyArgs @('--key', $key)
    }
}

function Invoke-Unread {
    param([string[]]$ArgsList)
    if ($ArgsList -and $ArgsList.Count -gt 0) { Die 'unread does not take arguments' }
    Invoke-Key -KeyArgs @('--key', 'alt+a')
}

function Invoke-ToggleMembers {
    param([string[]]$ArgsList)
    if ($ArgsList -and $ArgsList.Count -gt 0) { Die 'toggle-members does not take arguments' }
    Load-State
    $title = Get-WindowTitle
    if ($title -match '#') {
        if ([int]$Script:MEMBERS_OPEN -eq 1) {
            Invoke-ClickXy -X 908 -Y 36
            $Script:MEMBERS_OPEN = 0
        } else {
            Invoke-ClickXy -X 1124 -Y 36
            $Script:MEMBERS_OPEN = 1
        }
        Write-State
    } else {
        Invoke-Key -KeyArgs @('--key', 'ctrl+shift+m')
    }
    Delay-JumpField
}

function Invoke-FocusMembers {
    param([string[]]$ArgsList)
    if ($ArgsList -and $ArgsList.Count -gt 0) { Die 'focus-members does not take arguments' }
    Invoke-Key -KeyArgs @('--key', 'ctrl+shift+p')
}

function Invoke-Status {
    param([string[]]$ArgsList)
    if ($ArgsList -and $ArgsList.Count -gt 0) { Die 'status does not take arguments' }
    Invoke-Key -KeyArgs @('--key', 'ctrl+grave')
}

function Invoke-Connect {
    param([string[]]$ArgsList)
    if ($ArgsList -and $ArgsList.Count -gt 0) { Die 'connect does not take arguments' }
    Invoke-Key -KeyArgs @('--key', 'ctrl+comma')
}

function Get-ScreenshotMetrics {
    param([string]$Path)
    try {
        $out = & magick $Path -format '%[fx:standard_deviation] %k' info: 2>$null
        return [string]$out
    } catch {
        return ''
    }
}

function Test-ScreenshotNearSolid {
    param([string]$Path)
    if (-not (Test-Path -LiteralPath $Path) -or (Get-Item -LiteralPath $Path).Length -le 0) {
        return $true
    }
    $stats = Get-ScreenshotMetrics -Path $Path
    if (-not $stats) { return $true }
    $parts = $stats.Trim() -split '\s+'
    $stdToken = $parts[0]
    $colors = 0.0
    if ($parts.Count -gt 1) {
        $parsedColors = Parse-InvariantDouble -Token $parts[1]
        if ($null -ne $parsedColors) { $colors = [double]$parsedColors }
    }
    $token = $stdToken.Trim().ToLowerInvariant() -replace '^[+-]', ''
    if ($token -match '^(nan|inf)') { return $true }
    $std = Parse-InvariantDouble -Token $stdToken
    if ($null -eq $std) { return $true }
    if ([double]::IsNaN([double]$std) -or [double]$std -lt [double]$Script:SCREENSHOT_FLAT_STDDEV -or $colors -lt [double]$Script:SCREENSHOT_FLAT_COLORS) {
        return $true
    }
    return $false
}

function Invoke-Screenshot {
    param([string[]]$ShotArgs)
    $feature = $null; $name = $null
    $i = 0
    while ($i -lt $ShotArgs.Count) {
        switch ($ShotArgs[$i]) {
            '--feature' { $feature = $ShotArgs[$i + 1]; $i += 2 }
            '--name' { $name = $ShotArgs[$i + 1]; $i += 2 }
            default { Die "unknown screenshot argument: $($ShotArgs[$i])" }
        }
    }
    if (-not $feature -or -not $name) { Die 'screenshot requires --feature and --name' }
    Load-State
    $dest = Join-Path $Script:EVIDENCE_DIR $feature
    New-Item -ItemType Directory -Force -Path $dest | Out-Null
    $path = Join-Path $dest ($name + '.png')
    $bmp = Join-Path $dest ($name + '.bmp')
    for ($attempt = 1; $attempt -le [int]$Script:SCREENSHOT_GRAB_ATTEMPTS; $attempt++) {
        Remove-Item -LiteralPath $path -Force -ErrorAction SilentlyContinue
        Remove-Item -LiteralPath $bmp -Force -ErrorAction SilentlyContinue
        $ok = [OmaircDriver]::Capture($Script:HWND, $bmp)
        if ($ok -and (Test-Path -LiteralPath $bmp)) {
            & magick $bmp $path
            Remove-Item -LiteralPath $bmp -Force -ErrorAction SilentlyContinue
            if (-not (Test-ScreenshotNearSolid -Path $path)) {
                Write-Output $path
                return
            }
        } else {
            Remove-Item -LiteralPath $path, $bmp -Force -ErrorAction SilentlyContinue
        }
        if ($attempt -lt [int]$Script:SCREENSHOT_GRAB_ATTEMPTS) {
            Start-Sleep -Milliseconds ([int]([double]$Script:SCREENSHOT_GRAB_DELAY * 1000))
        }
    }
    $stats = Get-ScreenshotMetrics -Path $path
    $suffix = if ($stats) { " ($stats)" } else { '' }
    Die "screenshot is missing, empty, or still a flat frame: $path$suffix"
}

function Remap-VerifyPath {
    param([string]$Path)
    $p = $Path -replace '/', '\'
    if ($p -match '^test-artifacts\\verify\\(.*)$') {
        return (Join-Path $Script:ROOT (Join-Path 'test-artifacts\verify-desktop' $Matches[1]))
    }
    if ([System.IO.Path]::IsPathRooted($p)) { return $p }
    return (Join-Path $Script:ROOT $p)
}

function Invoke-Compare {
    param([string[]]$CmpArgs)
    $before = $null; $after = $null
    $i = 0
    while ($i -lt $CmpArgs.Count) {
        switch ($CmpArgs[$i]) {
            '--before' { $before = $CmpArgs[$i + 1]; $i += 2 }
            '--after' { $after = $CmpArgs[$i + 1]; $i += 2 }
            default { Die "unknown compare argument: $($CmpArgs[$i])" }
        }
    }
    if (-not $before -or -not $after) { Die 'compare requires --before and --after' }
    $before = Remap-VerifyPath -Path $before
    $after = Remap-VerifyPath -Path $after
    if (-not (Test-Path -LiteralPath $before) -or (Get-Item -LiteralPath $before).Length -le 0) {
        Die "missing before image: $before"
    }
    if (-not (Test-Path -LiteralPath $after) -or (Get-Item -LiteralPath $after).Length -le 0) {
        Die "missing after image: $after"
    }
    $difference = ''
    try {
        $difference = & magick compare -metric AE $before $after null: 2>&1 | Out-String
    } catch {
        $difference = [string]$_
    }
    $aeToken = ($difference.Trim() -split '\s+')[0]
    $parsedAe = Parse-InvariantDouble -Token $aeToken
    $ae = if ($null -ne $parsedAe) { [double]$parsedAe } else { 0.0 }
    if (-not ($ae -gt 100)) {
        Die "expected a visible UI change between $before and $after (AE=$($difference.Trim()))"
    }
    Write-Output "changed AE=$ae"
}

function Invoke-Cleanup {
    if (Test-Path -LiteralPath $Script:STATE_FILE) {
        try {
            $obj = Get-Content -LiteralPath $Script:STATE_FILE -Raw -Encoding UTF8 | ConvertFrom-Json
            if ($obj.APP_PID) { Stop-AppPid -ProcessId ([int]$obj.APP_PID) }
            if ($obj.STATE_DIR -and (Test-Path -LiteralPath ([string]$obj.STATE_DIR))) {
                Remove-Item -LiteralPath ([string]$obj.STATE_DIR) -Recurse -Force -ErrorAction SilentlyContinue
            }
        } catch { }
        Remove-Item -LiteralPath $Script:STATE_FILE -Force -ErrorAction SilentlyContinue
    }
    Write-Output 'cleaned isolated instance'
    Write-Output "evidence kept at $($Script:EVIDENCE_DIR)"
}

function ConvertFrom-ShellQuoteLine {
    param([string]$Line)
    $rest = $Line
    $tokens = New-Object System.Collections.Generic.List[string]
    while ($true) {
        $rest = $rest -replace '^[ \t]+', ''
        if ($rest.Length -eq 0) { break }
        if ($rest.StartsWith('#')) { break }
        $token = ''
        $mode = 'none'
        $skipToken = $false
        while ($rest.Length -gt 0) {
            $char = $rest.Substring(0, 1)
            $rest = $rest.Substring(1)
            if ($mode -eq 'none') {
                if ($char -eq ' ' -or $char -eq "`t") { break }
                elseif ($char -eq "'") { $mode = 'single' }
                elseif ($char -eq '"') { $mode = 'double' }
                elseif ($char -eq '#') {
                    if ($token.Length -eq 0) { $skipToken = $true; $rest = ''; break }
                    else { $token += $char }
                } else { $token += $char }
            } elseif ($mode -eq 'single') {
                if ($char -eq "'") { $mode = 'none' } else { $token += $char }
            } elseif ($mode -eq 'double') {
                if ($char -eq '"') { $mode = 'none' } else { $token += $char }
            }
        }
        if ($mode -ne 'none') { Die 'unbalanced quotes in desktop recipe' }
        if ($skipToken) { break }
        $tokens.Add($token)
    }
    return , $tokens.ToArray()
}

function Read-DesktopRecipe {
    param([string]$FeatureFile)
    $lines = Get-Content -LiteralPath $FeatureFile -Encoding UTF8
    $section = $false
    $inFence = $false
    $found = $false
    $closed = $false
    $recipe = New-Object System.Collections.Generic.List[string]
    foreach ($line in $lines) {
        if ($inFence) {
            if ($line -match '^```\s*$') { $inFence = $false; $closed = $true; continue }
            $recipe.Add($line)
            continue
        }
        if ($line -match '^## Driving it with control-omairc\s*$') { $section = $true; continue }
        if ($line -match '^## ') {
            if ($section) { $section = $false }
            continue
        }
        if ($section -and $line -match '^```desktop-recipe\s*$') {
            $inFence = $true
            $found = $true
            continue
        }
    }
    if (-not $found -or -not $closed) { return $null }
    return , $recipe.ToArray()
}

function Test-LaunchShouldSkip {
    param([string[]]$LaunchArgs)
    $flags = @()
    foreach ($a in $LaunchArgs) {
        if ($a -eq 'launch') { continue }
        $flags += $a
    }
    $wantDemo = $false
    foreach ($a in $flags) {
        if ($a -eq '--demo-server') { $wantDemo = $true }
        elseif ($a -eq '--visible') { }
        else { Die "unknown launch argument: $a. Use launch [--demo-server] [--visible]." }
    }
    $running = Get-DoctorDemoFlag
    if ($null -eq $running) { return $false }
    if ($wantDemo) {
        if ($running -ne 'yes') {
            Die 'isolated instance is already running without --demo-server. Refusing to cleanup a healthy instance.'
        }
    } else {
        if ($running -ne 'no') {
            Die 'isolated instance is already running with --demo-server. Refusing to cleanup a healthy instance.'
        }
    }
    [Console]::Error.WriteLine('launch skipped; isolated instance already matches')
    return $true
}

function Invoke-DriverCommand {
    param([string[]]$CmdArgs)
    if (-not $CmdArgs -or $CmdArgs.Count -eq 0) { Die 'empty command' }
    $cmd = $CmdArgs[0]
    $rest = @()
    if ($CmdArgs.Count -gt 1) { $rest = $CmdArgs[1..($CmdArgs.Count - 1)] }
    switch ($cmd) {
        'launch' { Invoke-Launch -LaunchArgs $rest }
        'doctor' { Invoke-Doctor }
        'title' { Invoke-Title }
        'wait-title' { Invoke-WaitTitle -WaitArgs $rest }
        'click' { Invoke-Click -ClickArgs $rest }
        'click-conversation' { Invoke-ClickConversation -ArgsList $rest }
        'click-member' { Invoke-ClickMember -ArgsList $rest }
        'click-people' { Invoke-ClickPeople -ArgsList $rest }
        'click-network' { Invoke-ClickNetwork }
        'click-edit' { Invoke-ClickEdit }
        'click-version' { Invoke-ClickVersion }
        'click-send' { Invoke-ClickSend }
        'focus-composer' { Invoke-FocusComposer -ArgsList $rest }
        'composer' { Invoke-Composer -ArgsList $rest }
        'jump' { Invoke-Jump -ArgsList $rest }
        'walk' { Invoke-Walk -WalkArgs $rest }
        'unread' { Invoke-Unread -ArgsList $rest }
        'toggle-members' { Invoke-ToggleMembers -ArgsList $rest }
        'focus-members' { Invoke-FocusMembers -ArgsList $rest }
        'nick-jump' { Invoke-NickJump -ArgsList $rest }
        'status' { Invoke-Status -ArgsList $rest }
        'connect' { Invoke-Connect -ArgsList $rest }
        'type' { Invoke-Type -TypeArgs $rest }
        'key' { Invoke-Key -KeyArgs $rest }
        'send' { Invoke-Send -SendArgs $rest }
        'screenshot' { Invoke-Screenshot -ShotArgs $rest }
        'compare' { Invoke-Compare -CmpArgs $rest }
        'qml-suite' { Reject-QtOnlyVerb -Verb 'qml-suite' }
        'doctor-qml' { Reject-QtOnlyVerb -Verb 'doctor-qml' }
        'cleanup' { Invoke-Cleanup }
        default { Die "unknown command '$cmd'. Try --help." }
    }
}

function Invoke-Run {
    param([string[]]$RunArgs)
    $feature = $null
    $dry = $false
    foreach ($a in $RunArgs) {
        if ($a -eq '--dry-run') { $dry = $true }
        elseif ($a -like '-*') { Die "unknown run argument: $a" }
        else {
            if ($feature) { Die "unexpected argument: $a" }
            $feature = $a
        }
    }
    if (-not $feature) { Die 'run requires a feature id' }
    if ($feature -notmatch '^[A-Za-z0-9_-]+$') { Die "invalid feature id '$feature'" }
    $file = Join-Path $Script:SCRIPT_DIR "..\verify-omairc\features\$feature.md"
    $file = [System.IO.Path]::GetFullPath($file)
    if (-not (Test-Path -LiteralPath $file)) { Die "no feature file for '$feature'" }
    $recipe = Read-DesktopRecipe -FeatureFile $file
    if ($null -eq $recipe) {
        Die "no desktop-recipe fence in $file. Put it under ## Driving it with control-omairc, before ## Gotchas."
    }
    $commands = New-Object System.Collections.Generic.List[string]
    foreach ($line in $recipe) {
        $trimmed = $line.Trim()
        if (-not $trimmed) { continue }
        if ($trimmed.StartsWith('#')) { continue }
        $verb = ($trimmed -split '\s+')[0]
        if ($verb -eq 'qml-suite' -or $verb -eq 'doctor-qml') {
            Die "desktop recipe cannot run '$verb': it is Qt-only (offscreen QML suite)"
        }
        $tokens = ConvertFrom-ShellQuoteLine -Line $trimmed
        if (-not $tokens -or $tokens.Count -eq 0) { Die "empty command in desktop recipe: $trimmed" }
        if ($tokens[0] -eq 'qml-suite' -or $tokens[0] -eq 'doctor-qml') {
            Die "desktop recipe cannot run '$($tokens[0])': it is Qt-only (offscreen QML suite)"
        }
        $commands.Add($trimmed)
    }
    if ($commands.Count -eq 0) { Die "desktop-recipe fence is empty in $file" }
    if ($dry) {
        $commands | ForEach-Object { Write-Output $_ }
        return
    }
    Push-Location $Script:ROOT
    try {
        $shots = New-Object System.Collections.Generic.List[string]
        foreach ($line in $commands) {
            $tokens = ConvertFrom-ShellQuoteLine -Line $line
            if (-not $tokens -or $tokens.Count -eq 0) { Die "empty command in desktop recipe: $line" }
            if ($tokens[0] -eq 'launch') {
                if (Test-LaunchShouldSkip -LaunchArgs $tokens) { continue }
            }
            if ($tokens[0] -eq 'screenshot') {
                $path = Invoke-Screenshot -ShotArgs $tokens[1..($tokens.Count - 1)]
                if (-not $path) { Die 'screenshot printed an empty path' }
                $path = [string](@($path) | Select-Object -Last 1)
                if (-not (Test-Path -LiteralPath $path) -or (Get-Item -LiteralPath $path).Length -le 0) {
                    Die "screenshot missing or empty: $path"
                }
                Write-Output $path
                $shots.Add($path)
            } else {
                Invoke-DriverCommand -CmdArgs $tokens | Out-Null
            }
        }
        foreach ($path in $shots) {
            if (-not $path) { Die 'screenshot path is empty' }
            if (-not (Test-Path -LiteralPath $path) -or (Get-Item -LiteralPath $path).Length -le 0) {
                Die "screenshot missing or empty: $path"
            }
        }
    } finally {
        Pop-Location
    }
}

# ---------------------------------------------------------------------------
# Dispatch
# ---------------------------------------------------------------------------

$argv = @($args)
if ($argv.Count -eq 0) {
    Show-Usage
    exit 0
}
$cmd = $argv[0]
$rest = @()
if ($argv.Count -gt 1) { $rest = $argv[1..($argv.Count - 1)] }

switch ($cmd) {
    'launch' { Invoke-Launch -LaunchArgs $rest }
    'doctor' { Invoke-Doctor }
    'title' { Invoke-Title }
    'wait-title' { Invoke-WaitTitle -WaitArgs $rest }
    'click' { Invoke-Click -ClickArgs $rest }
    'click-conversation' { Invoke-ClickConversation -ArgsList $rest }
    'click-member' { Invoke-ClickMember -ArgsList $rest }
    'click-people' { Invoke-ClickPeople -ArgsList $rest }
    'click-network' { Invoke-ClickNetwork }
    'click-edit' { Invoke-ClickEdit }
    'click-version' { Invoke-ClickVersion }
    'click-send' { Invoke-ClickSend }
    'focus-composer' { Invoke-FocusComposer -ArgsList $rest }
    'composer' { Invoke-Composer -ArgsList $rest }
    'jump' { Invoke-Jump -ArgsList $rest }
    'walk' { Invoke-Walk -WalkArgs $rest }
    'unread' { Invoke-Unread -ArgsList $rest }
    'toggle-members' { Invoke-ToggleMembers -ArgsList $rest }
    'focus-members' { Invoke-FocusMembers -ArgsList $rest }
    'nick-jump' { Invoke-NickJump -ArgsList $rest }
    'status' { Invoke-Status -ArgsList $rest }
    'connect' { Invoke-Connect -ArgsList $rest }
    'type' { Invoke-Type -TypeArgs $rest }
    'key' { Invoke-Key -KeyArgs $rest }
    'send' { Invoke-Send -SendArgs $rest }
    'screenshot' { Invoke-Screenshot -ShotArgs $rest }
    'compare' { Invoke-Compare -CmpArgs $rest }
    'run' { Invoke-Run -RunArgs $rest }
    'qml-suite' { Reject-QtOnlyVerb -Verb 'qml-suite' }
    'doctor-qml' { Reject-QtOnlyVerb -Verb 'doctor-qml' }
    'cleanup' { Invoke-Cleanup }
    { $_ -in @('-h', '--help', 'help') } { Show-Usage }
    default { Die "unknown command '$cmd'. Try --help." }
}
