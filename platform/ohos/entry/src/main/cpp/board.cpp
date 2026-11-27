#include <ace/xcomponent/native_interface_xcomponent.h>
#include <hilog/log.h>
#include <napi/native_api.h>
#include <native_window/external_window.h>

#include <cstdint>
#include <limits>
#include <map>
#include <memory>
#include <mutex>

extern "C" {
void *histeries_ohos_game_new();
void histeries_ohos_game_destroy(void *game);
int32_t histeries_ohos_end_turn(void *game);
void *histeries_ohos_create(void *nativeWindow);
int32_t histeries_ohos_present(void *renderer, const void *game, uint32_t width, uint32_t height);
void histeries_ohos_destroy(void *renderer);
}

namespace {
#include "board_lifecycle.inc"

OH_NativeXComponent_Callback callbacks = {onCreate, onChange, onDestroy, nullptr};

napi_value endTurn(napi_env env, napi_callback_info info)
{
    napi_value receiver = nullptr;
    void *native = nullptr;
    if (napi_get_cb_info(env, info, nullptr, nullptr, &receiver, nullptr) != napi_ok ||
        napi_unwrap(env, receiver, &native) != napi_ok || !native) {
        napi_throw_type_error(env, nullptr, "End turn requires a game board");
        return nullptr;
    }
    std::lock_guard lock(lifecycle);
    auto *board = static_cast<Board *>(native);
    const bool advanced = board->renderer && histeries_ohos_end_turn(board->game) == 0;
    if (advanced) {
        requestDraw(*board);
    }
    napi_value result = nullptr;
    napi_get_boolean(env, advanced, &result);
    return result;
}

void finalize(napi_env, void *data, void *)
{
    std::lock_guard lock(lifecycle);
    auto *board = static_cast<Board *>(data);
    if (findBoard(board->component) == board) {
        boards.erase(board->component);
    }
    // The component may already be gone. Its static callbacks cannot find an erased context.
    delete board;
}

napi_value initialize(napi_env env, napi_value exports)
{
    napi_value nativeObject = nullptr;
    void *native = nullptr;
    if (napi_get_named_property(env, exports, OH_NATIVE_XCOMPONENT_OBJ, &nativeObject) != napi_ok ||
        napi_unwrap(env, nativeObject, &native) != napi_ok || !native) {
        napi_throw_error(env, nullptr, "Missing native XComponent");
        return nullptr;
    }
    auto *component = static_cast<Component *>(native);
    auto board = std::unique_ptr<Board>(new Board{component, histeries_ohos_game_new()});
    if (!board->game || napi_wrap(env, exports, board.get(), finalize, nullptr, nullptr) != napi_ok) {
        napi_throw_error(env, nullptr, "Could not create board session");
        return nullptr;
    }
    {
        std::lock_guard lock(lifecycle);
        boards[component] = board.release();
    }

    napi_property_descriptor method = {"endTurn", nullptr, endTurn, nullptr, nullptr, nullptr, napi_default, nullptr};
    if (napi_define_properties(env, exports, 1, &method) != napi_ok ||
        OH_NativeXComponent_RegisterSurfaceShowCallback(component, onShow) != OH_NATIVEXCOMPONENT_RESULT_SUCCESS ||
        OH_NativeXComponent_RegisterSurfaceHideCallback(component, onHide) != OH_NATIVEXCOMPONENT_RESULT_SUCCESS ||
        OH_NativeXComponent_RegisterCallback(component, &callbacks) != OH_NATIVEXCOMPONENT_RESULT_SUCCESS) {
        napi_throw_error(env, nullptr, "Could not register board callbacks");
        return nullptr;
    }
    return exports;
}

napi_module module = {1, 0, nullptr, initialize, "histeries_board", nullptr, {nullptr}};
}

extern "C" __attribute__((constructor, visibility("default"))) void registerHisteriesBoard()
{
    napi_module_register(&module);
}
