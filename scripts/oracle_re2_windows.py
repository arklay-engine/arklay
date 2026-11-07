"""The Win32 side of scripts/oracle-re2.py: a debugger for the 32-bit game, its window's
picture (Windows.Graphics.Capture) and its sound (process loopback), with ctypes alone.

`oracle-re2.py` imports this on Windows only. Python is 64-bit and the game 32-bit, so the
debugger reads and writes the game's registers through `Wow64GetThreadContext` and
`Wow64SetThreadContext`, and its breakpoints and single steps arrive as the WOW64 exception
codes. See "On Windows" in oracle/re2.md.
"""

from __future__ import annotations

import ctypes
import ctypes.wintypes as wt
import os
import time
from pathlib import Path

kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)
user32 = ctypes.WinDLL("user32", use_last_error=True)
dwmapi = ctypes.WinDLL("dwmapi")

DEBUG_ONLY_THIS_PROCESS = 0x00000002
CREATE_UNICODE_ENVIRONMENT = 0x00000400
STARTF_USESHOWWINDOW = 0x00000001
SW_SHOWNOACTIVATE = 4
ERROR_SEM_TIMEOUT = 121

EXCEPTION_DEBUG_EVENT = 1
CREATE_THREAD_DEBUG_EVENT = 2
CREATE_PROCESS_DEBUG_EVENT = 3
EXIT_THREAD_DEBUG_EVENT = 4
EXIT_PROCESS_DEBUG_EVENT = 5
LOAD_DLL_DEBUG_EVENT = 6

DBG_CONTINUE = 0x00010002
DBG_EXCEPTION_NOT_HANDLED = 0x80010001

# A 32-bit thread's `int3` and trap flag arrive with the WOW64 codes; the 64-bit
# loader's own breakpoint with the native one.
BREAKPOINTS = (0x80000003, 0x4000001F)
SINGLE_STEPS = (0x80000004, 0x4000001E)
TRAP_FLAG = 0x100

WOW64_CONTEXT_CONTROL = 0x00010001
WOW64_CONTEXT_INTEGER = 0x00010002


class GameExited(Exception):
    pass


class EXCEPTION_RECORD(ctypes.Structure):
    _fields_ = [
        ("ExceptionCode", wt.DWORD),
        ("ExceptionFlags", wt.DWORD),
        ("ExceptionRecord", ctypes.c_void_p),
        ("ExceptionAddress", ctypes.c_void_p),
        ("NumberParameters", wt.DWORD),
        ("ExceptionInformation", ctypes.c_size_t * 15),
    ]


class EXCEPTION_DEBUG_INFO(ctypes.Structure):
    _fields_ = [("ExceptionRecord", EXCEPTION_RECORD), ("dwFirstChance", wt.DWORD)]


class CREATE_THREAD_DEBUG_INFO(ctypes.Structure):
    _fields_ = [("hThread", wt.HANDLE), ("lpThreadLocalBase", ctypes.c_void_p), ("lpStartAddress", ctypes.c_void_p)]


class CREATE_PROCESS_DEBUG_INFO(ctypes.Structure):
    _fields_ = [
        ("hFile", wt.HANDLE),
        ("hProcess", wt.HANDLE),
        ("hThread", wt.HANDLE),
        ("lpBaseOfImage", ctypes.c_void_p),
        ("dwDebugInfoFileOffset", wt.DWORD),
        ("nDebugInfoSize", wt.DWORD),
        ("lpThreadLocalBase", ctypes.c_void_p),
        ("lpStartAddress", ctypes.c_void_p),
        ("lpImageName", ctypes.c_void_p),
        ("fUnicode", wt.WORD),
    ]


class LOAD_DLL_DEBUG_INFO(ctypes.Structure):
    _fields_ = [
        ("hFile", wt.HANDLE),
        ("lpBaseOfDll", ctypes.c_void_p),
        ("dwDebugInfoFileOffset", wt.DWORD),
        ("nDebugInfoSize", wt.DWORD),
        ("lpImageName", ctypes.c_void_p),
        ("fUnicode", wt.WORD),
    ]


class DEBUG_EVENT_INFO(ctypes.Union):
    _fields_ = [
        ("Exception", EXCEPTION_DEBUG_INFO),
        ("CreateThread", CREATE_THREAD_DEBUG_INFO),
        ("CreateProcessInfo", CREATE_PROCESS_DEBUG_INFO),
        ("LoadDll", LOAD_DLL_DEBUG_INFO),
        ("ExitCode", wt.DWORD),
        ("raw", ctypes.c_byte * 176),
    ]


class DEBUG_EVENT(ctypes.Structure):
    _fields_ = [
        ("dwDebugEventCode", wt.DWORD),
        ("dwProcessId", wt.DWORD),
        ("dwThreadId", wt.DWORD),
        ("u", DEBUG_EVENT_INFO),
    ]


class WOW64_FLOATING_SAVE_AREA(ctypes.Structure):
    _fields_ = [(name, wt.DWORD) for name in (
        "ControlWord", "StatusWord", "TagWord", "ErrorOffset", "ErrorSelector", "DataOffset", "DataSelector",
    )] + [("RegisterArea", ctypes.c_byte * 80), ("Cr0NpxState", wt.DWORD)]


class WOW64_CONTEXT(ctypes.Structure):
    """A 32-bit thread's registers. `rax`, `rip` and `rsp` are the names the Linux backend's `Regs` uses."""

    _fields_ = (
        [(name, wt.DWORD) for name in ("ContextFlags", "Dr0", "Dr1", "Dr2", "Dr3", "Dr6", "Dr7")]
        + [("FloatSave", WOW64_FLOATING_SAVE_AREA)]
        + [(name, wt.DWORD) for name in (
            "SegGs", "SegFs", "SegEs", "SegDs", "Edi", "Esi", "Ebx", "Edx", "Ecx", "Eax", "Ebp", "Eip",
            "SegCs", "EFlags", "Esp", "SegSs",
        )]
        + [("ExtendedRegisters", ctypes.c_byte * 512)]
    )

    @property
    def rax(self) -> int:
        return self.Eax

    @rax.setter
    def rax(self, value: int) -> None:
        self.Eax = value & 0xFFFFFFFF

    @property
    def rip(self) -> int:
        return self.Eip

    @rip.setter
    def rip(self, value: int) -> None:
        self.Eip = value & 0xFFFFFFFF

    @property
    def rsp(self) -> int:
        return self.Esp


assert ctypes.sizeof(WOW64_CONTEXT) == 716


class STARTUPINFOW(ctypes.Structure):
    _fields_ = [
        ("cb", wt.DWORD), ("lpReserved", wt.LPWSTR), ("lpDesktop", wt.LPWSTR), ("lpTitle", wt.LPWSTR),
        ("dwX", wt.DWORD), ("dwY", wt.DWORD), ("dwXSize", wt.DWORD), ("dwYSize", wt.DWORD),
        ("dwXCountChars", wt.DWORD), ("dwYCountChars", wt.DWORD), ("dwFillAttribute", wt.DWORD),
        ("dwFlags", wt.DWORD), ("wShowWindow", wt.WORD), ("cbReserved2", wt.WORD),
        ("lpReserved2", ctypes.c_void_p), ("hStdInput", wt.HANDLE), ("hStdOutput", wt.HANDLE),
        ("hStdError", wt.HANDLE),
    ]


class PROCESS_INFORMATION(ctypes.Structure):
    _fields_ = [("hProcess", wt.HANDLE), ("hThread", wt.HANDLE), ("dwProcessId", wt.DWORD), ("dwThreadId", wt.DWORD)]


def _api(dll, name, restype, *argtypes):
    function = getattr(dll, name)
    function.restype = restype
    function.argtypes = argtypes
    return function


CreateProcessW = _api(
    kernel32, "CreateProcessW", wt.BOOL, wt.LPCWSTR, wt.LPWSTR, ctypes.c_void_p, ctypes.c_void_p, wt.BOOL,
    wt.DWORD, ctypes.c_void_p, wt.LPCWSTR, ctypes.POINTER(STARTUPINFOW), ctypes.POINTER(PROCESS_INFORMATION),
)
WaitForDebugEvent = _api(kernel32, "WaitForDebugEvent", wt.BOOL, ctypes.POINTER(DEBUG_EVENT), wt.DWORD)
ContinueDebugEvent = _api(kernel32, "ContinueDebugEvent", wt.BOOL, wt.DWORD, wt.DWORD, wt.DWORD)
DebugSetProcessKillOnExit = _api(kernel32, "DebugSetProcessKillOnExit", wt.BOOL, wt.BOOL)
ReadProcessMemory = _api(
    kernel32, "ReadProcessMemory", wt.BOOL, wt.HANDLE, ctypes.c_void_p, ctypes.c_void_p, ctypes.c_size_t,
    ctypes.POINTER(ctypes.c_size_t),
)
WriteProcessMemory = _api(
    kernel32, "WriteProcessMemory", wt.BOOL, wt.HANDLE, ctypes.c_void_p, ctypes.c_void_p, ctypes.c_size_t,
    ctypes.POINTER(ctypes.c_size_t),
)
FlushInstructionCache = _api(kernel32, "FlushInstructionCache", wt.BOOL, wt.HANDLE, ctypes.c_void_p, ctypes.c_size_t)
Wow64GetThreadContext = _api(kernel32, "Wow64GetThreadContext", wt.BOOL, wt.HANDLE, ctypes.POINTER(WOW64_CONTEXT))
Wow64SetThreadContext = _api(kernel32, "Wow64SetThreadContext", wt.BOOL, wt.HANDLE, ctypes.POINTER(WOW64_CONTEXT))
Wow64SuspendThread = _api(kernel32, "Wow64SuspendThread", wt.DWORD, wt.HANDLE)
ResumeThread = _api(kernel32, "ResumeThread", wt.DWORD, wt.HANDLE)
TerminateProcess = _api(kernel32, "TerminateProcess", wt.BOOL, wt.HANDLE, wt.UINT)
CloseHandle = _api(kernel32, "CloseHandle", wt.BOOL, wt.HANDLE)


def _fail(what: str) -> OSError:
    error = ctypes.get_last_error()
    return OSError(error, f"{what}: {ctypes.FormatError(error).strip()}")


class Debuggee:
    """The game, started under the debugger. It runs only between `next_exception` calls."""

    def __init__(self, exe: Path, cwd: Path) -> None:
        startup = STARTUPINFOW()
        startup.cb = ctypes.sizeof(startup)
        # Show the window without taking the focus from whatever the user is doing.
        startup.dwFlags = STARTF_USESHOWWINDOW
        startup.wShowWindow = SW_SHOWNOACTIVATE
        info = PROCESS_INFORMATION()
        command = ctypes.create_unicode_buffer(f'"{exe}"')
        # The game isn't DPI-aware, so on a scaled display Windows would stretch its window
        # and a picture of it would no longer be the game's own 640x480. Running it as if it
        # were aware keeps it at one pixel per pixel.
        variables = dict(os.environ, __COMPAT_LAYER="HighDpiAware")
        block = ctypes.create_unicode_buffer("".join(f"{k}={v}\0" for k, v in sorted(variables.items())) + "\0")
        if not CreateProcessW(
            str(exe), command, None, None, False, DEBUG_ONLY_THIS_PROCESS | CREATE_UNICODE_ENVIRONMENT,
            block, str(cwd), ctypes.byref(startup), ctypes.byref(info),
        ):
            raise _fail(f"CreateProcessW({exe})")
        # Our exit, however it comes, ends the game too.
        DebugSetProcessKillOnExit(True)
        self.process = info.hProcess
        self.pid = info.dwProcessId
        CloseHandle(info.hThread)
        self.threads: dict[int, int] = {}  # Thread id to the handle its debug event gave.
        self.pending: list[int] | None = None  # The event to continue: process, thread, status.
        self.exited = False

    def next_exception(self, timeout: float | None = None) -> tuple[int, int, int] | None:
        """Let the game run until a thread raises an exception, as (thread, code, address).

        The whole game stays stopped until the next call; `pass_exception` hands the exception
        to the game's own handlers instead of carrying on. None if `timeout` seconds pass first."""
        deadline = None if timeout is None else time.monotonic() + timeout
        event = DEBUG_EVENT()
        while True:
            self._continue()
            if self.exited:
                raise GameExited()
            # In short waits, so that Ctrl+C gets through.
            wait = 0.2 if deadline is None else max(0.0, min(0.2, deadline - time.monotonic()))
            if not WaitForDebugEvent(ctypes.byref(event), int(wait * 1000)):
                if ctypes.get_last_error() != ERROR_SEM_TIMEOUT:
                    raise _fail("WaitForDebugEvent")
                if deadline is not None and time.monotonic() >= deadline:
                    return None
                continue
            self.pending = [event.dwProcessId, event.dwThreadId, DBG_CONTINUE]
            code = event.dwDebugEventCode
            if code == EXCEPTION_DEBUG_EVENT:
                record = event.u.Exception.ExceptionRecord
                return event.dwThreadId, record.ExceptionCode, record.ExceptionAddress or 0
            if code == CREATE_PROCESS_DEBUG_EVENT:
                info = event.u.CreateProcessInfo
                if info.hFile:
                    CloseHandle(info.hFile)
                self.threads[event.dwThreadId] = info.hThread
            elif code == CREATE_THREAD_DEBUG_EVENT:
                self.threads[event.dwThreadId] = event.u.CreateThread.hThread
            elif code == EXIT_THREAD_DEBUG_EVENT:
                self.threads.pop(event.dwThreadId, None)
            elif code == LOAD_DLL_DEBUG_EVENT:
                if event.u.LoadDll.hFile:
                    CloseHandle(event.u.LoadDll.hFile)
            elif code == EXIT_PROCESS_DEBUG_EVENT:
                self.exited = True

    def pass_exception(self) -> None:
        """Give the exception just reported to the game's own handlers."""
        if self.pending:
            self.pending[2] = DBG_EXCEPTION_NOT_HANDLED

    def release(self) -> None:
        """Continue the event just reported without waiting for the next."""
        self._continue()

    def _continue(self) -> None:
        if self.pending:
            process, thread, status = self.pending
            self.pending = None
            if not ContinueDebugEvent(process, thread, status):
                raise _fail("ContinueDebugEvent")

    def read(self, address: int, size: int) -> bytes:
        buffer = ctypes.create_string_buffer(size)
        done = ctypes.c_size_t()
        if not ReadProcessMemory(self.process, address, buffer, size, ctypes.byref(done)) or done.value != size:
            raise _fail(f"ReadProcessMemory({address:#x}, {size})")
        return buffer.raw

    def write(self, address: int, data: bytes) -> None:
        done = ctypes.c_size_t()
        if not WriteProcessMemory(self.process, address, data, len(data), ctypes.byref(done)) or done.value != len(data):
            raise _fail(f"WriteProcessMemory({address:#x}, {len(data)})")
        FlushInstructionCache(self.process, address, len(data))

    def get_context(self, tid: int) -> WOW64_CONTEXT:
        context = WOW64_CONTEXT()
        context.ContextFlags = WOW64_CONTEXT_CONTROL | WOW64_CONTEXT_INTEGER
        if not Wow64GetThreadContext(self.threads[tid], ctypes.byref(context)):
            raise _fail("Wow64GetThreadContext")
        return context

    def set_context(self, tid: int, context: WOW64_CONTEXT) -> None:
        if not Wow64SetThreadContext(self.threads[tid], ctypes.byref(context)):
            raise _fail("Wow64SetThreadContext")

    def suspend(self, tid: int) -> None:
        if Wow64SuspendThread(self.threads[tid]) == 0xFFFFFFFF:
            raise _fail("SuspendThread")

    def resume(self, tid: int) -> None:
        if ResumeThread(self.threads[tid]) == 0xFFFFFFFF:
            raise _fail("ResumeThread")

    def terminate(self) -> None:
        """End the game and wait for it to go."""
        if not self.exited:
            TerminateProcess(self.process, 1)
            deadline = time.monotonic() + 10
            try:
                while not self.exited and time.monotonic() < deadline:
                    self.next_exception(0.5)
            except GameExited:
                pass
            self._continue()
        CloseHandle(self.process)


# ---------------------------------------------------------------- the window

WNDENUMPROC = ctypes.WINFUNCTYPE(wt.BOOL, wt.HWND, wt.LPARAM)
DWMWA_EXTENDED_FRAME_BOUNDS = 9

EnumWindows = _api(user32, "EnumWindows", wt.BOOL, WNDENUMPROC, wt.LPARAM)
GetWindowThreadProcessId = _api(user32, "GetWindowThreadProcessId", wt.DWORD, wt.HWND, ctypes.POINTER(wt.DWORD))
IsWindowVisible = _api(user32, "IsWindowVisible", wt.BOOL, wt.HWND)
IsIconic = _api(user32, "IsIconic", wt.BOOL, wt.HWND)
GetClientRect = _api(user32, "GetClientRect", wt.BOOL, wt.HWND, ctypes.POINTER(wt.RECT))
ClientToScreen = _api(user32, "ClientToScreen", wt.BOOL, wt.HWND, ctypes.POINTER(wt.POINT))
DwmFlush = _api(dwmapi, "DwmFlush", ctypes.c_long)
DwmGetWindowAttribute = _api(dwmapi, "DwmGetWindowAttribute", ctypes.c_long, wt.HWND, wt.DWORD, ctypes.c_void_p, wt.DWORD)
DwmSetWindowAttribute = _api(dwmapi, "DwmSetWindowAttribute", ctypes.c_long, wt.HWND, wt.DWORD, ctypes.c_void_p, wt.DWORD)
DWMWA_WINDOW_CORNER_PREFERENCE = 33
DWMWCP_DONOTROUND = 1

# Our coordinates are the display's own pixels, whatever its scaling.
user32.SetProcessDpiAwarenessContext(ctypes.c_void_p(-4))  # DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2


def game_window(pid: int) -> int | None:
    """The process's visible top-level window with the largest client area."""
    found: list[tuple[int, int]] = []

    def visit(hwnd, _):
        owner = wt.DWORD()
        GetWindowThreadProcessId(hwnd, ctypes.byref(owner))
        if owner.value == pid and IsWindowVisible(hwnd):
            rect = wt.RECT()
            GetClientRect(hwnd, ctypes.byref(rect))
            found.append((rect.right * rect.bottom, hwnd))
        return True

    EnumWindows(WNDENUMPROC(visit), 0)
    return max(found)[1] if found else None


def client_size(hwnd: int) -> tuple[int, int]:
    rect = wt.RECT()
    GetClientRect(hwnd, ctypes.byref(rect))
    return rect.right, rect.bottom


# ---------------------------------------------------------------- COM, by hand

combase = ctypes.OleDLL("combase")
d3d11 = ctypes.OleDLL("d3d11")
E_NOINTERFACE = 0x80004002 - (1 << 32)


class GUID(ctypes.Structure):
    _fields_ = [("Data1", wt.DWORD), ("Data2", wt.WORD), ("Data3", wt.WORD), ("Data4", ctypes.c_ubyte * 8)]

    @classmethod
    def parse(cls, text: str) -> "GUID":
        import uuid

        return cls.from_buffer_copy(uuid.UUID(text).bytes_le)


def _method(interface, index: int, *argtypes, restype=ctypes.HRESULT):
    """The `index`th method of a COM interface pointer's vtable, bound to it."""
    vtable = ctypes.cast(interface, ctypes.POINTER(ctypes.POINTER(ctypes.c_void_p))).contents
    function = ctypes.WINFUNCTYPE(restype, ctypes.c_void_p, *argtypes)(vtable[index])
    return lambda *args: function(interface, *args)


def _release(interface) -> None:
    if interface:
        _method(interface, 2, restype=wt.ULONG)()


def _query(interface, iid: "GUID") -> ctypes.c_void_p:
    out = ctypes.c_void_p()
    _method(interface, 0, ctypes.POINTER(GUID), ctypes.POINTER(ctypes.c_void_p))(ctypes.byref(iid), ctypes.byref(out))
    return out


def _factory(name: str, iid: "GUID") -> ctypes.c_void_p:
    hstring = ctypes.c_void_p()
    combase.WindowsCreateString(ctypes.c_wchar_p(name), len(name), ctypes.byref(hstring))
    try:
        out = ctypes.c_void_p()
        combase.RoGetActivationFactory(hstring, ctypes.byref(iid), ctypes.byref(out))
        return out
    finally:
        combase.WindowsDeleteString(hstring)


IID_IDXGIDevice = GUID.parse("54ec77fa-1377-44e6-8c32-88fd5f44c84c")
IID_IGraphicsCaptureItemInterop = GUID.parse("3628E81B-3CAC-4C60-B7F4-23CE0E0C3356")
IID_IGraphicsCaptureItem = GUID.parse("79C3F95B-31F7-4EC2-A464-632EF5D30760")
IID_IDirect3D11CaptureFramePoolStatics2 = GUID.parse("589B103F-6BBC-5DF5-A991-02E28B3B66D5")
IID_IGraphicsCaptureSession3 = GUID.parse("F2CDD966-22AE-5EA1-9596-3A289344C3BE")
IID_IGraphicsCaptureSession2 = GUID.parse("2C39AE40-7D2E-5044-804E-8B6799D4CF9E")
IID_IClosable = GUID.parse("30D5A829-7FA4-4026-83BB-D75BAE4EA99E")
IID_IDirect3DDxgiInterfaceAccess = GUID.parse("A9B3D012-3DF2-4EE3-B8D1-8695F457D3C1")
IID_ID3D11Texture2D = GUID.parse("6f15aaf2-d208-4e89-9ab4-489535d34f9c")


class SizeInt32(ctypes.Structure):
    _fields_ = [("Width", ctypes.c_int32), ("Height", ctypes.c_int32)]


class D3D11_TEXTURE2D_DESC(ctypes.Structure):
    _fields_ = [
        ("Width", wt.UINT), ("Height", wt.UINT), ("MipLevels", wt.UINT), ("ArraySize", wt.UINT),
        ("Format", wt.UINT), ("SampleCount", wt.UINT), ("SampleQuality", wt.UINT), ("Usage", wt.UINT),
        ("BindFlags", wt.UINT), ("CPUAccessFlags", wt.UINT), ("MiscFlags", wt.UINT),
    ]


class D3D11_MAPPED_SUBRESOURCE(ctypes.Structure):
    _fields_ = [("pData", ctypes.c_void_p), ("RowPitch", wt.UINT), ("DepthPitch", wt.UINT)]


def _close(inspectable) -> None:
    """`IClosable.Close`, then the reference."""
    if inspectable:
        closable = _query(inspectable, IID_IClosable)
        _method(closable, 6)()
        _release(closable)
        _release(inspectable)


STALE_PICTURES = 300
QueryPerformanceCounter = _api(kernel32, "QueryPerformanceCounter", wt.BOOL, ctypes.POINTER(ctypes.c_int64))
QueryPerformanceFrequency = _api(kernel32, "QueryPerformanceFrequency", wt.BOOL, ctypes.POINTER(ctypes.c_int64))


def performance_time() -> int:
    """Now, in the 100 ns units of the performance counter that capture frames are stamped in."""
    count, frequency = ctypes.c_int64(), ctypes.c_int64()
    QueryPerformanceCounter(ctypes.byref(count))
    QueryPerformanceFrequency(ctypes.byref(frequency))
    return count.value * 10_000_000 // frequency.value
ES_CONTINUOUS = 0x80000000
ES_SYSTEM_REQUIRED = 0x00000001
ES_DISPLAY_REQUIRED = 0x00000002
SetThreadExecutionState = _api(kernel32, "SetThreadExecutionState", wt.DWORD, wt.DWORD)


def keep_awake(display: bool) -> None:
    """Keep the machine (and the display, for pictures) from sleeping until `let_sleep`."""
    SetThreadExecutionState(ES_CONTINUOUS | ES_SYSTEM_REQUIRED | (ES_DISPLAY_REQUIRED if display else 0))


def let_sleep() -> None:
    SetThreadExecutionState(ES_CONTINUOUS)


class WindowCapture:
    """The window's picture as DWM composes it, through Windows.Graphics.Capture.

    The capture takes the window's own surface, so other windows (or the system's) over it
    don't change the picture, and it asks nothing of the window's thread, which is stopped
    at the breakpoint when a picture is taken. A minimised window has no picture."""

    def __init__(self, hwnd: int) -> None:
        self.hwnd = hwnd
        # Windows 11 rounds a window's corners, which cuts the client area's bottom corners
        # off the picture. Not every Windows lets another process change that.
        corners = ctypes.c_int(DWMWCP_DONOTROUND)
        self.rounded = DwmSetWindowAttribute(
            hwnd, DWMWA_WINDOW_CORNER_PREFERENCE, ctypes.byref(corners), ctypes.sizeof(corners)
        ) != 0
        self.device = self.context = self.winrt_device = self.item = self.pool = self.session = None
        self.frame = None  # The newest frame the pool has given, held until a newer one comes.
        self.new_frame = False  # Whether it came since the last picture.
        self.frame_time = 0  # When DWM composed `frame`, in 100 ns units of the performance counter.
        self.stop_time = 0  # When the game last stopped (`stopped`), the same way.
        # Pictures in a row with no new frame. The game presents every tick but in a door's
        # hold (Wine's route has 106 identical frames in a row at most), so many more mean
        # that DWM has stopped composing: the display is off or the session locked.
        self.unchanged = 0
        self.staging = None
        self.staging_size = (0, 0)
        try:
            combase.RoInitialize(1)  # multithreaded
        except OSError:
            pass  # already initialised on this thread
        device, context = ctypes.c_void_p(), ctypes.c_void_p()
        # A hardware device with BGRA support, the SDK version 7.
        d3d11.D3D11CreateDevice(None, 1, None, 0x20, None, 0, 7, ctypes.byref(device), None, ctypes.byref(context))
        self.device, self.context = device, context
        dxgi = _query(device, IID_IDXGIDevice)
        winrt_device = ctypes.c_void_p()
        d3d11.CreateDirect3D11DeviceFromDXGIDevice(dxgi, ctypes.byref(winrt_device))
        _release(dxgi)
        self.winrt_device = winrt_device
        interop = _factory("Windows.Graphics.Capture.GraphicsCaptureItem", IID_IGraphicsCaptureItemInterop)
        item = ctypes.c_void_p()
        _method(interop, 3, wt.HWND, ctypes.POINTER(GUID), ctypes.POINTER(ctypes.c_void_p))(
            hwnd, ctypes.byref(IID_IGraphicsCaptureItem), ctypes.byref(item)
        )
        _release(interop)
        self.item = item
        size = SizeInt32()
        _method(item, 7, ctypes.POINTER(SizeInt32))(ctypes.byref(size))
        statics = _factory("Windows.Graphics.Capture.Direct3D11CaptureFramePool", IID_IDirect3D11CaptureFramePoolStatics2)
        pool = ctypes.c_void_p()
        # B8G8R8A8UIntNormalized, two buffers.
        _method(statics, 6, ctypes.c_void_p, ctypes.c_int, ctypes.c_int32, SizeInt32, ctypes.POINTER(ctypes.c_void_p))(
            winrt_device, 87, 3, size, ctypes.byref(pool)
        )
        _release(statics)
        self.pool = pool
        session = ctypes.c_void_p()
        _method(pool, 10, ctypes.c_void_p, ctypes.POINTER(ctypes.c_void_p))(item, ctypes.byref(session))
        self.session = session
        for iid, value in ((IID_IGraphicsCaptureSession2, 0), (IID_IGraphicsCaptureSession3, 0)):
            # No cursor, and no border round the window where the system allows it.
            extra = ctypes.c_void_p()
            try:
                _method(session, 0, ctypes.POINTER(GUID), ctypes.POINTER(ctypes.c_void_p))(
                    ctypes.byref(iid), ctypes.byref(extra)
                )
                _method(extra, 7, wt.BOOLEAN)(value)
            except OSError:
                pass
            finally:
                _release(extra)
        _method(session, 6)()

    def drain(self) -> None:
        """Keep only the newest frame: the pool holds three, and stops giving more while full."""
        try_next = _method(self.pool, 7, ctypes.POINTER(ctypes.c_void_p))
        while True:
            frame = ctypes.c_void_p()
            try_next(ctypes.byref(frame))
            if not frame:
                return
            _close(self.frame)
            self.frame = frame
            self.new_frame = True
            when = ctypes.c_int64()
            _method(frame, 7, ctypes.POINTER(ctypes.c_int64))(ctypes.byref(when))
            self.frame_time = when.value

    def stopped(self) -> None:
        """The game has just stopped: take what the pool holds, and note when."""
        self.stop_time = performance_time()
        self.drain()

    def picture(self) -> tuple[int, int, bytes]:
        """The client area of the newest frame, as (width, height, BGRX rows top down)."""
        if IsIconic(self.hwnd):
            raise RuntimeError("the game's window is minimised, so it has no picture: restore it")
        # The game presented its frame just before it stopped: let DWM compose it, and the
        # capture hand it over.
        DwmFlush()
        DwmFlush()
        # The game presents and reaches `Pad_set` within microseconds, and DWM composes the
        # frame at the next refresh, so the frame of this tick is composed after the stop and
        # comes a little after that. A frame composed before the stop is the last tick's, or
        # this one's if the game presented nothing new (in a door's hold): wait a while for a
        # newer one, then take it.
        self.drain()
        deadline = time.monotonic() + 0.1
        while self.frame_time < self.stop_time and time.monotonic() < deadline:
            time.sleep(0.002)
            self.drain()
        if not self.frame:
            raise RuntimeError("the capture has no frame of the game's window yet")
        self.unchanged = 0 if self.new_frame else self.unchanged + 1
        self.new_frame = False
        if self.unchanged > STALE_PICTURES:
            raise RuntimeError(
                f"the game's window has shown nothing new for {self.unchanged} pictures: is the display off, "
                "or the session locked?"
            )
        surface = ctypes.c_void_p()
        _method(self.frame, 6, ctypes.POINTER(ctypes.c_void_p))(ctypes.byref(surface))
        access = _query(surface, IID_IDirect3DDxgiInterfaceAccess)
        texture = ctypes.c_void_p()
        _method(access, 3, ctypes.POINTER(GUID), ctypes.POINTER(ctypes.c_void_p))(
            ctypes.byref(IID_ID3D11Texture2D), ctypes.byref(texture)
        )
        _release(access)
        _release(surface)
        try:
            desc = D3D11_TEXTURE2D_DESC()
            _method(texture, 10, ctypes.POINTER(D3D11_TEXTURE2D_DESC), restype=None)(ctypes.byref(desc))
            if self.staging_size != (desc.Width, desc.Height):
                _release(self.staging)
                staging = D3D11_TEXTURE2D_DESC(desc.Width, desc.Height, 1, 1, desc.Format, 1, 0, 3, 0, 0x20000, 0)
                self.staging = ctypes.c_void_p()
                _method(self.device, 5, ctypes.POINTER(D3D11_TEXTURE2D_DESC), ctypes.c_void_p,
                        ctypes.POINTER(ctypes.c_void_p))(ctypes.byref(staging), None, ctypes.byref(self.staging))
                self.staging_size = (desc.Width, desc.Height)
            _method(self.context, 47, ctypes.c_void_p, ctypes.c_void_p, restype=None)(self.staging, texture)
        finally:
            _release(texture)
        # Where the client area is in the frame, which is the window's visible bounds.
        bounds = wt.RECT()
        DwmGetWindowAttribute(self.hwnd, DWMWA_EXTENDED_FRAME_BOUNDS, ctypes.byref(bounds), ctypes.sizeof(bounds))
        origin = wt.POINT(0, 0)
        ClientToScreen(self.hwnd, ctypes.byref(origin))
        width, height = client_size(self.hwnd)
        left, top = origin.x - bounds.left, origin.y - bounds.top
        if left < 0 or top < 0 or left + width > self.staging_size[0] or top + height > self.staging_size[1]:
            raise RuntimeError("the game's client area isn't inside its captured frame")
        mapped = D3D11_MAPPED_SUBRESOURCE()
        _method(self.context, 14, ctypes.c_void_p, wt.UINT, ctypes.c_int, wt.UINT,
                ctypes.POINTER(D3D11_MAPPED_SUBRESOURCE))(self.staging, 0, 1, 0, ctypes.byref(mapped))
        try:
            rows = [
                ctypes.string_at(mapped.pData + (top + y) * mapped.RowPitch + left * 4, width * 4)
                for y in range(height)
            ]
        finally:
            _method(self.context, 15, ctypes.c_void_p, wt.UINT, restype=None)(self.staging, 0)
        return width, height, b"".join(rows)

    def close(self) -> None:
        _close(self.frame)
        _close(self.session)
        _close(self.pool)
        for interface in (self.staging, self.item, self.winrt_device, self.context, self.device):
            _release(interface)
        self.frame = self.session = self.pool = self.staging = self.item = None


# ---------------------------------------------------------------- the sound
#
# The game's own audio, and nothing else's, through process loopback capture: the audio
# engine's `VAD\Process_Loopback` device, activated for the game's process tree (Windows 10
# 2004 and later). It is the counterpart of the private PulseAudio sink the Wine path uses.

ole32 = ctypes.OleDLL("ole32")
mmdevapi = ctypes.OleDLL("mmdevapi")

VT_BLOB = 65
AUDIOCLIENT_ACTIVATION_TYPE_PROCESS_LOOPBACK = 1
PROCESS_LOOPBACK_MODE_INCLUDE_TARGET_PROCESS_TREE = 0
AUDCLNT_SHAREMODE_SHARED = 0
AUDCLNT_STREAMFLAGS_LOOPBACK = 0x00020000
AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM = 0x80000000
AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY = 0x08000000
AUDCLNT_BUFFERFLAGS_SILENT = 0x2
COINIT_MULTITHREADED = 0x0
IID_IUnknown = GUID.parse("00000000-0000-0000-C000-000000000046")
IID_IAgileObject = GUID.parse("94ea2b94-e9cc-49e0-c0ff-ee64ca8f5b90")
IID_IActivateAudioInterfaceCompletionHandler = GUID.parse("41D949AB-9862-444A-80F6-C261334DA5EB")
IID_IAudioClient = GUID.parse("1CB9AD4C-DBFA-4c32-B178-C2F568A703B2")
IID_IAudioCaptureClient = GUID.parse("C8ADBD64-E71E-48a0-A4DE-185C395CD317")


class WAVEFORMATEX(ctypes.Structure):
    _fields_ = [
        ("wFormatTag", wt.WORD), ("nChannels", wt.WORD), ("nSamplesPerSec", wt.DWORD),
        ("nAvgBytesPerSec", wt.DWORD), ("nBlockAlign", wt.WORD), ("wBitsPerSample", wt.WORD), ("cbSize", wt.WORD),
    ]


class PROCESS_LOOPBACK_PARAMS(ctypes.Structure):
    _fields_ = [("ActivationType", ctypes.c_int), ("TargetProcessId", wt.DWORD), ("ProcessLoopbackMode", ctypes.c_int)]


class BLOB(ctypes.Structure):
    _fields_ = [("cbSize", wt.ULONG), ("pBlobData", ctypes.c_void_p)]


class PROPVARIANT(ctypes.Structure):
    _fields_ = [("vt", wt.WORD), ("reserved", wt.WORD * 3), ("blob", BLOB)]


class _CompletionHandler:
    """A static `IActivateAudioInterfaceCompletionHandler` that is also agile, so the audio
    engine may call it from any thread. Python owns it, so it doesn't count references."""

    def __init__(self) -> None:
        import threading

        self.done = threading.Event()
        qi = ctypes.WINFUNCTYPE(ctypes.HRESULT, ctypes.c_void_p, ctypes.POINTER(GUID), ctypes.POINTER(ctypes.c_void_p))
        ref = ctypes.WINFUNCTYPE(wt.ULONG, ctypes.c_void_p)
        completed = ctypes.WINFUNCTYPE(ctypes.HRESULT, ctypes.c_void_p, ctypes.c_void_p)
        known = [bytes(iid) for iid in (IID_IUnknown, IID_IAgileObject, IID_IActivateAudioInterfaceCompletionHandler)]

        def query(this, iid, out):
            if bytes(iid.contents) in known:
                out[0] = this
                return 0
            out[0] = None
            return E_NOINTERFACE

        def activate_completed(this, operation):
            self.done.set()
            return 0

        self.functions = [qi(query), ref(lambda this: 1), ref(lambda this: 1), completed(activate_completed)]
        self.vtable = (ctypes.c_void_p * 4)(*[ctypes.cast(f, ctypes.c_void_p) for f in self.functions])
        self.object = ctypes.c_void_p(ctypes.addressof(self.vtable))
        self.pointer = ctypes.addressof(self.object)


class SoundRecorder:
    """Write what process `pid` (and its children) plays to a 16-bit stereo 44.1 kHz WAV.

    The loopback stream has nothing while the game is silent, so each packet is placed by its
    timestamp, with silence filling the gaps: the file's time is wall-clock time from
    `started` (a `time.monotonic()` value), as the `.ticks` log needs."""

    RATE = 44100

    def __init__(self, pid: int, path: str) -> None:
        import threading
        import wave

        self.wav = wave.open(path, "wb")
        self.wav.setnchannels(2)
        self.wav.setsampwidth(2)
        self.wav.setframerate(self.RATE)
        self.pid = pid
        self.written = 0  # Frames written.
        self.origin = time.perf_counter()
        self.started = time.monotonic()
        self.error: BaseException | None = None
        self.running = True
        ready = threading.Event()
        self.thread = threading.Thread(target=self._run, args=(ready,), daemon=True)
        self.thread.start()
        if not ready.wait(10):
            self.close()
            raise RuntimeError("process loopback capture did not start")
        if self.error:
            self.close()
            raise RuntimeError(f"process loopback capture failed: {self.error}")

    def _activate(self) -> ctypes.c_void_p:
        params = PROCESS_LOOPBACK_PARAMS(
            AUDIOCLIENT_ACTIVATION_TYPE_PROCESS_LOOPBACK, self.pid, PROCESS_LOOPBACK_MODE_INCLUDE_TARGET_PROCESS_TREE
        )
        variant = PROPVARIANT()
        variant.vt = VT_BLOB
        variant.blob.cbSize = ctypes.sizeof(params)
        variant.blob.pBlobData = ctypes.addressof(params)
        handler = _CompletionHandler()
        operation = ctypes.c_void_p()
        mmdevapi.ActivateAudioInterfaceAsync(
            ctypes.c_wchar_p("VAD\\Process_Loopback"), ctypes.byref(IID_IAudioClient), ctypes.byref(variant),
            ctypes.c_void_p(handler.pointer), ctypes.byref(operation),
        )
        try:
            if not handler.done.wait(10):
                raise RuntimeError("the audio engine did not answer")
            result = ctypes.c_long()
            client = ctypes.c_void_p()
            _method(operation, 3, ctypes.POINTER(ctypes.c_long), ctypes.POINTER(ctypes.c_void_p))(
                ctypes.byref(result), ctypes.byref(client)
            )
            if result.value < 0:
                raise OSError(result.value, "activating the process loopback device")
            return client
        finally:
            _release(operation)

    def _run(self, ready) -> None:
        client = capture = ctypes.c_void_p()
        try:
            ole32.CoInitializeEx(None, COINIT_MULTITHREADED)
            client = self._activate()
            form = WAVEFORMATEX(1, 2, self.RATE, self.RATE * 4, 4, 16, 0)
            _method(client, 3, ctypes.c_int, wt.DWORD, ctypes.c_longlong, ctypes.c_longlong,
                    ctypes.POINTER(WAVEFORMATEX), ctypes.c_void_p)(
                AUDCLNT_SHAREMODE_SHARED,
                AUDCLNT_STREAMFLAGS_LOOPBACK | AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM
                | AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY,
                2_000_000, 0, ctypes.byref(form), None,
            )
            capture = ctypes.c_void_p()
            _method(client, 14, ctypes.POINTER(GUID), ctypes.POINTER(ctypes.c_void_p))(
                ctypes.byref(IID_IAudioCaptureClient), ctypes.byref(capture)
            )
            get_buffer = _method(
                capture, 3, ctypes.POINTER(ctypes.c_void_p), ctypes.POINTER(ctypes.c_uint32),
                ctypes.POINTER(wt.DWORD), ctypes.POINTER(ctypes.c_uint64), ctypes.POINTER(ctypes.c_uint64),
            )
            release_buffer = _method(capture, 4, ctypes.c_uint32)
            next_packet = _method(capture, 5, ctypes.POINTER(ctypes.c_uint32))
            _method(client, 10)()
            ready.set()
            size = ctypes.c_uint32()
            data = ctypes.c_void_p()
            frames = ctypes.c_uint32()
            flags = wt.DWORD()
            position = ctypes.c_uint64()
            when = ctypes.c_uint64()
            while self.running:
                next_packet(ctypes.byref(size))
                while size.value:
                    get_buffer(ctypes.byref(data), ctypes.byref(frames), ctypes.byref(flags),
                               ctypes.byref(position), ctypes.byref(when))
                    count = frames.value
                    if flags.value & AUDCLNT_BUFFERFLAGS_SILENT or not data:
                        chunk = bytes(count * 4)
                    else:
                        chunk = ctypes.string_at(data, count * 4)
                    release_buffer(count)
                    # `when` is in 100 ns units of the performance counter, which
                    # `time.perf_counter` reads.
                    at = round((when.value / 1e7 - self.origin) * self.RATE)
                    if at > self.written:
                        self.wav.writeframesraw(bytes((at - self.written) * 4))
                        self.written = at
                    self.wav.writeframesraw(chunk)
                    self.written += count
                    next_packet(ctypes.byref(size))
                time.sleep(0.005)
            _method(client, 11)()
        except BaseException as error:  # handed to the main thread
            self.error = error
            ready.set()
        finally:
            _release(capture)
            _release(client)

    def close(self) -> None:
        self.running = False
        self.thread.join(5)
        # Silence to the end, so the file covers the whole run.
        end = round((time.perf_counter() - self.origin) * self.RATE)
        if end > self.written:
            self.wav.writeframesraw(bytes((end - self.written) * 4))
        self.wav.close()
