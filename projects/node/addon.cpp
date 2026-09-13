// Stable Node-API only: strings cross one synchronous, bounded Rust ABI call.
// No Node/V8 internals, global handles, object pointers or retained JS state.
#include <node_api.h>
#include <seele-core.h>
#include <memory>
#include <string>
#include <stdexcept>

namespace {
bool check(napi_env env, napi_status status) {
  if (status == napi_ok) return true;
  bool pending = false;
  napi_is_exception_pending(env, &pending);
  if (!pending) napi_throw_error(env, nullptr, "Native value conversion failed");
  return false;
}
napi_value call(napi_env env, napi_callback_info info) {
  try {
    size_t count = 2;
    napi_value arguments[2]{};
    if (!check(env, napi_get_cb_info(env, info, &count, arguments, nullptr, nullptr))) return nullptr;
    napi_valuetype type;
    if (count != 1 || !check(env, napi_typeof(env, arguments[0], &type))) {
      if (count != 1) napi_throw_type_error(env, nullptr, "Expected one JSON string");
      return nullptr;
    }
    if (type != napi_string) {
      napi_throw_type_error(env, nullptr, "Expected one JSON string");
      return nullptr;
    }
    size_t length = 0;
    if (!check(env, napi_get_value_string_utf8(env, arguments[0], nullptr, 0, &length))) return nullptr;
    if (length > seele_core_max_message()) {
      napi_throw_range_error(env, nullptr, "Native request exceeds its byte limit");
      return nullptr;
    }
    // Node appends NUL; explicit byte lengths preserve embedded NUL correctly.
    std::string input(length + 1, '\0');
    size_t written = 0;
    if (!check(env, napi_get_value_string_utf8(env, arguments[0], input.data(), input.size(), &written))) return nullptr;
    if (written != length) {
      napi_throw_error(env, nullptr, "Native request conversion changed its length");
      return nullptr;
    }
    SeeleBytes output = seele_qml_call(reinterpret_cast<const uint8_t *>(input.data()), length);
    const auto release = [](SeeleBytes *value) { seele_qml_free(*value); };
    const std::unique_ptr<SeeleBytes, decltype(release)> guard(&output, release);
    if (!output.data || output.length > seele_core_max_message()) {
      napi_throw_error(env, nullptr, "Invalid native response");
      return nullptr;
    }
    napi_value result;
    if (!check(env, napi_create_string_utf8(env, reinterpret_cast<const char *>(output.data), output.length, &result))) return nullptr;
    return result;
  } catch (const std::exception &) {
    napi_throw_error(env, nullptr, "Native allocation failed");
    return nullptr;
  } catch (...) {
    napi_throw_error(env, nullptr, "Native binding failed");
    return nullptr;
  }
}
napi_value initialize(napi_env env, napi_value exports) {
  napi_value function;
  if (!check(env, napi_create_function(env, "evaluate", NAPI_AUTO_LENGTH, call, nullptr, &function)) ||
      !check(env, napi_set_named_property(env, exports, "evaluate", function))) return nullptr;
  return exports;
}
}
NAPI_MODULE(seele_core, initialize)
