// Verify the visible window in screen coordinates, independently of GPU staging readbacks.
#include "frame_abi.h"
#include "frame_present.h"
#include <stdexcept>
#include <cstdio>
#include <future>

namespace tuxbox_test {
namespace {
void pump() {
    MSG message;
    while (PeekMessageW(&message, nullptr, 0, 0, PM_REMOVE)) {
        TranslateMessage(&message);
        DispatchMessageW(&message);
    }
}

// Sample points in percent of the client area: four corners, then the centre.
constexpr std::array<POINT, 5> samples{{{5, 5}, {95, 5}, {5, 95}, {95, 95}, {50, 50}}};
// build.py writes the package splash beside the splash-mode guest: a 64x36
// image in this colour.
constexpr COLORREF splash_image = RGB(0x30, 0xc0, 0x90);
constexpr LONG splash_width = 64, splash_height = 36;

std::string describe(const std::array<COLORREF, samples.size()>& pixels) {
    char text[80];
    std::snprintf(text, sizeof(text), " screen=%#lx/%#lx/%#lx/%#lx/%#lx", pixels[0], pixels[1], pixels[2],
                  pixels[3], pixels[4]);
    return text;
}
}

DisplayCheck::DisplayCheck(UINT width, UINT height, const wchar_t* window_mode)
    : check_size_(window_mode != nullptr), splash_(window_mode && !wcscmp(window_mode, L"splash")) {
    if (window_mode && !wcscmp(window_mode, L"owned")) return;
    if (splash_) {
        // Like ProjectScarlett: an ANSI class, and a window for the whole output.
        WNDCLASSEXA title{};
        title.cbSize = sizeof(title);
        title.lpfnWndProc = DefWindowProcA;
        title.hInstance = GetModuleHandleW(nullptr);
        title.lpszClassName = "TuxBoxSplashTitle";
        if (!RegisterClassExA(&title)) throw std::runtime_error("could not register the title window class");
        RECT frame{0, 0, static_cast<LONG>(width), static_cast<LONG>(height)};
        AdjustWindowRect(&frame, WS_OVERLAPPEDWINDOW, FALSE);
        window_ = CreateWindowExA(0, title.lpszClassName, "Project TuxBox splash check", WS_OVERLAPPEDWINDOW,
                                  CW_USEDEFAULT, CW_USEDEFAULT, frame.right - frame.left, frame.bottom - frame.top,
                                  nullptr, nullptr, title.hInstance, nullptr);
        if (!window_) throw std::runtime_error("could not create the title window");
        owns_title_ = true;
        // The provider fits the window as it is created, and keeps it transparent
        // until its first frame: only the splash shows.
        RECT client{};
        POINT origin{};
        measure(client, origin);
        if (!transparent()) throw std::runtime_error("the title window shows behind the splash" + sizing_);
        ShowWindow(window_, SW_SHOW);
        UpdateWindow(window_);
        const auto deadline = GetTickCount64() + 2000;
        while (!(splash_window_ = FindWindowW(L"TuxBox.Splash", nullptr)) && GetTickCount64() < deadline) {
            pump();
            Sleep(10);
        }
        if (!splash_window_) throw std::runtime_error("no package splash window" + sizing_);
        RECT bounds{};
        MONITORINFO monitor{};
        monitor.cbSize = sizeof(monitor);
        GetWindowRect(splash_window_, &bounds);
        GetMonitorInfoW(MonitorFromWindow(window_, MONITOR_DEFAULTTONEAREST), &monitor);
        const auto& area = monitor.rcWork;
        const auto style = GetWindowLongW(splash_window_, GWL_STYLE), extended = GetWindowLongW(splash_window_, GWL_EXSTYLE);
        if (bounds.right - bounds.left != splash_width || bounds.bottom - bounds.top != splash_height ||
            std::abs(bounds.left + bounds.right - area.left - area.right) > 1 ||
            std::abs(bounds.top + bounds.bottom - area.top - area.bottom) > 1)
            throw std::runtime_error("the package splash is not the image, centred" + sizing_);
        if ((style & WS_CAPTION) || !(extended & WS_EX_TOOLWINDOW) || !(extended & WS_EX_NOACTIVATE))
            throw std::runtime_error("the package splash has a frame, a taskbar entry or takes the focus");
        if (!splash_shows(4000)) throw std::runtime_error("the package splash did not appear" + describe(sample()));
        return;
    }
    UINT client_width = width * 6, client_height = height * 6;
    if (check_size_) {
        client_width = !wcscmp(window_mode, L"fit") ? 960 : width;
        client_height = !wcscmp(window_mode, L"fit") ? 540 : height;
    }
    // The popup's client area fills the window. Keep it above other windows while sampling.
    window_ = CreateWindowExW(WS_EX_TOPMOST, L"STATIC", L"Project TuxBox GPU check",
                              WS_POPUP, 24, 24, client_width, client_height, nullptr, nullptr,
                              GetModuleHandleW(nullptr), nullptr);
    if (!window_) throw std::runtime_error("could not create presentation window");
    owns_title_ = true;
    ShowWindow(window_, SW_SHOW);
    SetActiveWindow(window_);
    UpdateWindow(window_);
    pump();
}
DisplayCheck::~DisplayCheck() { if (owns_title_) DestroyWindow(window_); }

void DisplayCheck::measure(RECT& client, POINT& origin) {
    if (!GetClientRect(window_, &client)) throw std::runtime_error("could not get presentation client area");
    origin = {};
    POINT end{client.right, client.bottom};
    if (!ClientToScreen(window_, &origin) || !ClientToScreen(window_, &end))
        throw std::runtime_error("could not locate presentation window");
    if (check_size_) {
        MONITORINFO monitor{};
        monitor.cbSize = sizeof(monitor);
        if (!GetMonitorInfoW(MonitorFromWindow(window_, MONITOR_DEFAULTTONEAREST), &monitor))
            throw std::runtime_error("could not locate display work area");
        const auto& area = monitor.rcWork;
        sizing_ = " client=" + std::to_string(client.right) + "x" + std::to_string(client.bottom)
            + " origin=" + std::to_string(origin.x) + "," + std::to_string(origin.y)
            + " monitor=" + std::to_string(area.right - area.left) + "x" + std::to_string(area.bottom - area.top);
        if (origin.x < area.left || origin.y < area.top || end.x > area.right || end.y > area.bottom)
            throw std::runtime_error("off-screen presentation" + sizing_);
        if (std::abs(client.right * 9 - client.bottom * 16) > 16)
            throw std::runtime_error("distorted presentation" + sizing_);
    }
}

std::array<COLORREF, 5> DisplayCheck::sample() const {
    std::array<COLORREF, samples.size()> pixels{};
    RECT client{};
    POINT origin{};
    if (!GetClientRect(window_, &client) || !ClientToScreen(window_, &origin)) return pixels;
    HDC screen = GetDC(nullptr);
    if (!screen) throw std::runtime_error("could not acquire screen DC");
    for (size_t i = 0; i < pixels.size(); ++i)
        pixels[i] = GetPixel(screen, origin.x + client.right * samples[i].x / 100,
                             origin.y + client.bottom * samples[i].y / 100);
    ReleaseDC(nullptr, screen);
    return pixels;
}

bool DisplayCheck::shows(COLORREF corner, COLORREF center, ULONGLONG milliseconds) const {
    const auto deadline = GetTickCount64() + milliseconds;
    do {
        pump();
        const auto pixels = sample();
        bool matched = true;
        for (size_t i = 0; i < pixels.size(); ++i) matched &= pixels[i] == (i == 4 ? center : corner);
        if (matched) return true;
        Sleep(16);
    } while (GetTickCount64() < deadline);
    return false;
}

bool DisplayCheck::transparent() const {
    BYTE alpha = 255;
    DWORD flags = 0;
    return (GetWindowLongW(window_, GWL_EXSTYLE) & WS_EX_LAYERED) &&
           GetLayeredWindowAttributes(window_, nullptr, &alpha, &flags) && (flags & LWA_ALPHA) && alpha == 0;
}

bool DisplayCheck::splash_shows(ULONGLONG milliseconds) const {
    const auto deadline = GetTickCount64() + milliseconds;
    do {
        pump();
        RECT bounds{};
        if (!IsWindow(splash_window_) || !GetWindowRect(splash_window_, &bounds)) return false;
        HDC screen = GetDC(nullptr);
        if (!screen) throw std::runtime_error("could not acquire screen DC");
        bool matched = true;
        for (const auto& at : samples) {
            // The sample points, moved inside the splash's own rectangle.
            const LONG x = bounds.left + (bounds.right - bounds.left) * (at.x / 2 + 25) / 100;
            const LONG y = bounds.top + (bounds.bottom - bounds.top) * (at.y / 2 + 25) / 100;
            matched &= GetPixel(screen, x, y) == splash_image;
        }
        ReleaseDC(nullptr, screen);
        if (matched) return true;
        Sleep(16);
    } while (GetTickCount64() < deadline);
    return false;
}

void DisplayCheck::verify(ID3D12CommandQueue* queue, ID3D12Resource* source, UINT64 token,
                          COLORREF corner, COLORREF center) {
    struct FullPlane {
        frame_abi::Plane prefix;
        UINT options[12];
    } plane{{token, 1, 0, &source}, {}};
    struct Parameters {
        float threshold;
        UINT options[7];
    } params{token == 1 ? 10.f : 0.f, {}};
    static_assert(sizeof(FullPlane) == 72 && sizeof(Parameters) == 32);
    const auto present = frame_abi::method<frame_abi::Present>(queue, 30);
    if (token == 1) {
        params.options[5] = 1;
        if (present(queue, 1, &plane.prefix, &params) != E_NOTIMPL)
            throw std::runtime_error("unsupported presentation flag was accepted");
        params.options[5] = 0;
        // Bytes 24 and 32 of the 72-byte retail plane are the source-rectangle and
        // destination pointers; the provider dereferences them. Probe the color
        // space at byte 40 instead, a plain value it must reject when nonzero.
        plane.options[4] = 1;
        if (present(queue, 1, &plane.prefix, &params) != E_NOTIMPL)
            throw std::runtime_error("unsupported plane option was accepted");
        plane.options[4] = 0;
    }
    // Frosty's main thread owns the game window while a render thread presents.
    const auto submit = [&] {
        auto render = std::async(std::launch::async, [&] { return present(queue, 1, &plane.prefix, &params); });
        while (render.wait_for(std::chrono::milliseconds(1)) != std::future_status::ready) pump();
        // The present may resize the title window asynchronously just before it returns; apply
        // that move before the client area is measured.
        pump();
        const HRESULT hr = render.get();
        if (hr != S_OK) {
            char message[96];
            std::snprintf(message, sizeof(message), "queue slot 30 PresentX returned %#lx", static_cast<unsigned long>(hr));
            throw std::runtime_error(message);
        }
    };
    if (splash_ && token == 1) {
        // A lone first frame completes behind the splash, which stays on the screen
        // while the title's window stays transparent.
        submit();
        const auto until = GetTickCount64() + 300;
        while (GetTickCount64() < until) {
            if (!splash_shows(0) || !transparent()) throw std::runtime_error("the first frame ended the package splash");
            Sleep(16);
        }
        return;
    }
    if (splash_) {
        // With the held first frame, three more within a second are steady and end the splash.
        for (int i = 0; i < 3; ++i) submit();
    }
    submit();
    if (!window_) {
        EnumWindows([](HWND w, LPARAM data) -> BOOL {
            DWORD pid{};
            GetWindowThreadProcessId(w, &pid);
            if (pid != GetCurrentProcessId() || !IsWindowVisible(w) || GetWindow(w, GW_OWNER)) return TRUE;
            *reinterpret_cast<HWND*>(data) = w;
            return FALSE;
        }, reinterpret_cast<LPARAM>(&window_));
    }
    RECT client{};
    POINT origin{};
    measure(client, origin);
    const auto deadline = GetTickCount64() + 4000;
    // GE-Proton11-14 can leave a presented image off the screen until a later
    // present, which a title supplies every frame. Repeat this one a few times.
    auto resubmit_at = GetTickCount64() + 250;
    int resubmits = 3;
    std::array<COLORREF, samples.size()> pixels{};
    do {
        pump();
        pixels = sample();
        bool matched = true;
        for (size_t i = 0; i < pixels.size(); ++i) matched &= pixels[i] == (i == 4 ? center : corner);
        if (matched) {
            if (splash_) {
                if (IsWindow(splash_window_)) throw std::runtime_error("the package splash outlived steady frames");
                // The provider takes the layering off once this thread takes its message.
                const auto until = GetTickCount64() + 2000;
                while ((GetWindowLongW(window_, GWL_EXSTYLE) & WS_EX_LAYERED) && GetTickCount64() < until) {
                    pump();
                    Sleep(10);
                }
                if (GetWindowLongW(window_, GWL_EXSTYLE) & WS_EX_LAYERED)
                    throw std::runtime_error("the title window kept the splash's layering");
                // Frames after that still reach the screen.
                for (int i = 0; i < 10; ++i) {
                    submit();
                    Sleep(16);
                }
                if (!shows(corner, center, 2000))
                    throw std::runtime_error("frames stopped reaching the screen after the splash" + describe(sample()));
                sizing_ += " splash=held,ended";
            }
            return;
        }
        if (resubmits && GetTickCount64() >= resubmit_at) {
            submit();
            --resubmits;
            resubmit_at += 250;
        }
        Sleep(16);
    } while (GetTickCount64() < deadline);
    char message[180];
    std::snprintf(message, sizeof(message), "screen colors on frame %llu: %#lx/%#lx/%#lx/%#lx/%#lx, expected %#lx border and %#lx center",
                  token, pixels[0], pixels[1], pixels[2], pixels[3], pixels[4], corner, center);
    throw std::runtime_error(message + sizing_);
}
}
