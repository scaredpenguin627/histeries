// Host regression fixture. Drive the same XComponent callbacks as ArkUI, with
// deterministic size, renderer and display-tick callbacks in place of the SDK.
#include <cassert>
#include <cstdint>
#include <limits>
#include <map>
#include <mutex>

namespace {
struct OH_NativeXComponent {};
struct OHNativeWindow {};
using Tick = void (*)(OH_NativeXComponent *, uint64_t, uint64_t);
constexpr int LOG_APP = 0, LOG_INFO = 1, LOG_ERROR = 2, OH_NATIVEXCOMPONENT_RESULT_SUCCESS = 0;
constexpr int GET_BUFFER_GEOMETRY = 1;

struct Fixture {
    int game, renderer;
    OHNativeWindow window;
    bool retained = false, rendererLive = false, failCreate = false, loseDevice = false;
    int refs = 0, unrefs = 0, creates = 0, destroys = 0, presents = 0, gameDrops = 0;
    uint32_t componentWidth = 200, componentHeight = 160;
    int32_t bufferWidth = 1280, bufferHeight = 720;
    bool hasBufferGeometry = false;
    uint32_t presentedWidth = 0, presentedHeight = 0;
    Tick tick = nullptr;
} fixture;

void OH_LOG_Print(int, int, unsigned int, const char *, const char *, ...) {}

int OH_NativeWindow_NativeObjectReference(void *window)
{
    assert(window == &fixture.window && !fixture.retained);
    fixture.retained = true;
    ++fixture.refs;
    return 0;
}

int OH_NativeWindow_NativeObjectUnreference(void *window)
{
    assert(window == &fixture.window && fixture.retained && !fixture.rendererLive);
    fixture.retained = false;
    ++fixture.unrefs;
    return 0;
}

int OH_NativeXComponent_GetXComponentSize(OH_NativeXComponent *, void *window, uint64_t *width, uint64_t *height)
{
    assert(window == &fixture.window && fixture.retained);
    *width = fixture.componentWidth;
    *height = fixture.componentHeight;
    return 0;
}

int OH_NativeWindow_NativeWindowHandleOpt(OHNativeWindow *window, int option, int32_t *height, int32_t *width)
{
    assert(window == &fixture.window && fixture.retained && option == GET_BUFFER_GEOMETRY);
    if (!fixture.hasBufferGeometry) {
        return -1;
    }
    *height = fixture.bufferHeight;
    *width = fixture.bufferWidth;
    return 0;
}

int OH_NativeXComponent_RegisterOnFrameCallback(OH_NativeXComponent *, Tick tick)
{
    assert(!fixture.tick && tick);
    fixture.tick = tick;
    return 0;
}

int OH_NativeXComponent_UnregisterOnFrameCallback(OH_NativeXComponent *)
{
    assert(fixture.tick);
    fixture.tick = nullptr;
    return 0;
}

void *histeries_ohos_create(void *window)
{
    assert(window == &fixture.window && fixture.retained && !fixture.rendererLive);
    ++fixture.creates;
    fixture.rendererLive = !fixture.failCreate;
    return fixture.rendererLive ? &fixture.renderer : nullptr;
}

void histeries_ohos_destroy(void *renderer)
{
    assert(renderer == &fixture.renderer && fixture.rendererLive);
    fixture.rendererLive = false;
    ++fixture.destroys;
}

int32_t histeries_ohos_present(void *renderer, const void *game, uint32_t width, uint32_t height)
{
    assert(renderer == &fixture.renderer && fixture.rendererLive && game == &fixture.game && fixture.gameDrops == 0);
    fixture.presentedWidth = width;
    fixture.presentedHeight = height;
    ++fixture.presents;
    if (fixture.loseDevice) {
        fixture.loseDevice = false;
        return 2;
    }
    return 0;
}

void histeries_ohos_game_destroy(void *game)
{
    assert(game == &fixture.game && !fixture.rendererLive && !fixture.retained);
    ++fixture.gameDrops;
}

#include "../entry/src/main/cpp/board_lifecycle.inc"

void drainTicks(Component *component)
{
    // Fail promptly if a callback remains registered after its retry budget.
    for (int i = 0; fixture.tick && i < 20; ++i) {
        fixture.tick(component, 0, 0);
    }
    assert(!fixture.tick);
}
}

int main()
{
    Component component;
    {
        Board board{&component, &fixture.game};
        boards.emplace(&component, &board);
        auto *window = &fixture.window;
        onCreate(&component, window);
        assert(fixture.creates == 1 && fixture.presents == 1 && fixture.tick);
        assert(fixture.presentedWidth == 200 && fixture.presentedHeight == 160);
        const Tick lateTick = fixture.tick;

        onHide(&component, window);
        assert(!fixture.tick && !fixture.rendererLive && fixture.retained && fixture.gameDrops == 0);
        lateTick(&component, 0, 0);
        onChange(&component, window);
        assert(fixture.presents == 1 && fixture.creates == 1);

        fixture.hasBufferGeometry = true;
        onShow(&component, window);
        assert(fixture.presentedWidth == 1280 && fixture.presentedHeight == 720);
        drainTicks(&component);
        assert(fixture.creates == 2 && fixture.presents == 4);

        fixture.loseDevice = true;
        onChange(&component, window);
        assert(!fixture.rendererLive && fixture.tick && fixture.presents == 5);
        drainTicks(&component);
        assert(fixture.creates == 3 && fixture.presents == 8 && fixture.rendererLive);

        onHide(&component, window);
        fixture.failCreate = true;
        onShow(&component, window);
        drainTicks(&component);
        lateTick(&component, 0, 0);
        assert(fixture.creates == 6 && fixture.presents == 8 && !fixture.rendererLive);
        fixture.failCreate = false;
        onHide(&component, window);
        onShow(&component, window);
        drainTicks(&component);
        assert(fixture.creates == 7 && fixture.presents == 11);

        fixture.loseDevice = true;
        fixture.failCreate = true;
        onChange(&component, window);
        drainTicks(&component);
        assert(fixture.creates == 10 && fixture.presents == 12 && !fixture.rendererLive);
        fixture.failCreate = false;
        fixture.bufferWidth = 1024;
        onChange(&component, window);
        assert(fixture.creates == 11 && fixture.presents == 13 && fixture.tick);
        assert(fixture.presentedWidth == 1024 && fixture.presentedHeight == 720);

        onDestroy(&component, window);
        lateTick(&component, 0, 0);
        onChange(&component, window);
        onShow(&component, window);
        onDestroy(&component, window);
        assert(!fixture.tick && !fixture.rendererLive && !fixture.retained);
        assert(fixture.destroys == 5 && fixture.refs == 1 && fixture.unrefs == 1 && fixture.gameDrops == 0);
        assert(fixture.presents == 13);
        boards.erase(&component);
    }
    assert(fixture.gameDrops == 1);
}
