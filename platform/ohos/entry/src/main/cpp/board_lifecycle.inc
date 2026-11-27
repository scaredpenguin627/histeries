constexpr unsigned int domain = 0x0001;
constexpr char tag[] = "Histeries";
using Component = OH_NativeXComponent;

// ArkUI owns the component. Its exports object owns the game and can outlive a surface.
struct Board {
    Component *component;
    void *game;
    void *window = nullptr;
    void *renderer = nullptr;
    bool visible = false;
    bool firstFrame = true;
    bool waitingForFrame = false;
    unsigned int submissionsLeft = 0;
    unsigned int retriesLeft = 0;
    unsigned int creationsLeft = 0;

    void releaseRenderer()
    {
        if (renderer) {
            histeries_ohos_destroy(renderer);
            renderer = nullptr;
        }
        firstFrame = true;
    }

    void releaseSurface()
    {
        releaseRenderer();
        if (window) {
            OH_NativeWindow_NativeObjectUnreference(window);
            window = nullptr;
        }
        visible = false;
        submissionsLeft = 0;
    }

    ~Board()
    {
        releaseSurface();
        if (game) {
            histeries_ohos_game_destroy(game);
        }
    }
};

std::mutex lifecycle;
std::map<Component *, Board *> boards;

Board *findBoard(Component *component)
{
    const auto it = boards.find(component);
    return it == boards.end() ? nullptr : it->second;
}

void onFrame(Component *, uint64_t, uint64_t);

void setFrameCallback(Board &board, bool needed)
{
    if (needed == board.waitingForFrame) {
        return;
    }
    const auto status = needed ? OH_NativeXComponent_RegisterOnFrameCallback(board.component, onFrame)
                               : OH_NativeXComponent_UnregisterOnFrameCallback(board.component);
    if (status == OH_NATIVEXCOMPONENT_RESULT_SUCCESS) {
        board.waitingForFrame = needed;
    } else {
        OH_LOG_Print(LOG_APP, LOG_ERROR, domain, tag, "Frame callback registration failed (%{public}d)", status);
    }
}

int32_t submit(Board &board, uint64_t &width, uint64_t &height)
{
    int32_t bufferWidth = 0;
    int32_t bufferHeight = 0;
    auto *nativeWindow = static_cast<OHNativeWindow *>(board.window);
    // GET_BUFFER_GEOMETRY returns height first. The initial buffer may not exist until configuration.
    if (OH_NativeWindow_NativeWindowHandleOpt(nativeWindow, GET_BUFFER_GEOMETRY, &bufferHeight, &bufferWidth) == 0 &&
        bufferWidth > 0 && bufferHeight > 0) {
        width = static_cast<uint32_t>(bufferWidth);
        height = static_cast<uint32_t>(bufferHeight);
    } else {
        if (!board.firstFrame ||
            OH_NativeXComponent_GetXComponentSize(board.component, board.window, &width, &height) !=
                OH_NATIVEXCOMPONENT_RESULT_SUCCESS ||
            width == 0 || height == 0 || width > std::numeric_limits<uint32_t>::max() ||
            height > std::numeric_limits<uint32_t>::max()) {
            return 1;
        }
    }

    if (!board.renderer) {
        if (board.creationsLeft == 0) {
            return 2;
        }
        --board.creationsLeft;
        board.renderer = histeries_ohos_create(board.window);
        if (!board.renderer) {
            return 2;
        }
    }
    return histeries_ohos_present(board.renderer, board.game, static_cast<uint32_t>(width),
                                 static_cast<uint32_t>(height));
}

void draw(Board &board)
{
    uint64_t width = 0;
    uint64_t height = 0;
    const auto result = submit(board, width, height);
    if (result == 2) {
        board.releaseRenderer();
    }
    if (result == 0) {
        --board.submissionsLeft;
        if (board.firstFrame) {
            OH_LOG_Print(LOG_APP, LOG_INFO, domain, tag, "Vulkan frame submitted for presentation %{public}ux%{public}u",
                         static_cast<unsigned int>(width), static_cast<unsigned int>(height));
            board.firstFrame = false;
        }
    } else if (result == 1 && board.retriesLeft > 0) {
        --board.retriesLeft;
    } else if (result != 2 || board.creationsLeft == 0) {
        board.submissionsLeft = 0;
        OH_LOG_Print(LOG_APP, LOG_ERROR, domain, tag, "Frame submission failed (%{public}d)", result);
    }
    setFrameCallback(board, board.submissionsLeft > 0);
}

void requestDraw(Board &board)
{
    if (!board.visible || !board.window) {
        return;
    }
    // Vulkan can discard a queued presentation without reporting that failure through wgpu.
    // Give this update two further display ticks and bound acquisition retries for an unavailable surface.
    board.submissionsLeft = 3;
    board.retriesLeft = 8;
    board.creationsLeft = 3;
    draw(board);
}

void onFrame(Component *component, uint64_t, uint64_t)
{
    std::lock_guard lock(lifecycle);
    auto *board = findBoard(component);
    if (board && board->waitingForFrame && board->submissionsLeft > 0) {
        draw(*board);
    }
}

void onCreate(Component *component, void *window)
{
    std::lock_guard lock(lifecycle);
    auto *board = findBoard(component);
    if (!board || !window) {
        return;
    }
    setFrameCallback(*board, false);
    board->releaseSurface();
    if (OH_NativeWindow_NativeObjectReference(window) != 0) {
        OH_LOG_Print(LOG_APP, LOG_ERROR, domain, tag, "Could not retain the XComponent window");
        return;
    }
    board->window = window;
    board->visible = true;
    requestDraw(*board);
}

void onChange(Component *component, void *window)
{
    std::lock_guard lock(lifecycle);
    auto *board = findBoard(component);
    if (board && board->window == window) {
        requestDraw(*board);
    }
}

void onDestroy(Component *component, void *window)
{
    std::lock_guard lock(lifecycle);
    auto *board = findBoard(component);
    if (board && board->window == window) {
        setFrameCallback(*board, false);
        board->releaseSurface();
    }
}

void onHide(Component *component, void *window)
{
    std::lock_guard lock(lifecycle);
    auto *board = findBoard(component);
    if (board && board->window == window) {
        board->visible = false;
        board->submissionsLeft = 0;
        setFrameCallback(*board, false);
        board->releaseRenderer();
    }
}

void onShow(Component *component, void *window)
{
    std::lock_guard lock(lifecycle);
    auto *board = findBoard(component);
    if (board && board->window == window) {
        board->visible = true;
        requestDraw(*board);
    }
}
