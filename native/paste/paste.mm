#import <AppKit/AppKit.h>
#import <ApplicationServices/ApplicationServices.h>

#include <node_api.h>


namespace {

napi_value BoolValue(napi_env env, bool value) {
  napi_value result = nullptr;
  napi_get_boolean(env, value, &result);
  return result;
}

napi_value NullValue(napi_env env) {
  napi_value result = nullptr;
  napi_get_null(env, &result);
  return result;
}

bool ReadPid(napi_env env, napi_callback_info info, pid_t* pid) {
  size_t argc = 1;
  napi_value args[1] = {nullptr};
  if (napi_get_cb_info(env, info, &argc, args, nullptr, nullptr) != napi_ok) return false;
  if (argc < 1) return false;
  int32_t value = 0;
  if (napi_get_value_int32(env, args[0], &value) != napi_ok) return false;
  *pid = static_cast<pid_t>(value);
  return true;
}

/** Process id + name of the frontmost application, or null. */
napi_value Frontmost(napi_env env, napi_callback_info /*info*/) {
  @autoreleasepool {
    NSRunningApplication* application = [[NSWorkspace sharedWorkspace] frontmostApplication];
    if (application == nil) return NullValue(env);

    NSString* name = application.localizedName ?: @"";
    napi_value result = nullptr;
    napi_value pid = nullptr;
    napi_value localizedName = nullptr;
    if (napi_create_object(env, &result) != napi_ok ||
        napi_create_int32(env, application.processIdentifier, &pid) != napi_ok ||
        napi_create_string_utf8(env, name.UTF8String, NAPI_AUTO_LENGTH, &localizedName) != napi_ok) {
      return NullValue(env);
    }
    napi_set_named_property(env, result, "pid", pid);
    napi_set_named_property(env, result, "name", localizedName);
    return result;
  }
}

napi_value BundleId(napi_env env, napi_callback_info /*info*/) {
  @autoreleasepool {
    NSString* identifier = [[NSBundle mainBundle] bundleIdentifier] ?: @"";
    napi_value result = nullptr;
    napi_create_string_utf8(env, identifier.UTF8String, NAPI_AUTO_LENGTH, &result);
    return result;
  }
}

napi_value AccessibilityTrusted(napi_env env, napi_callback_info /*info*/) {
  return BoolValue(env, AXIsProcessTrusted());
}

napi_value PostEventTrusted(napi_env env, napi_callback_info /*info*/) {
  return BoolValue(env, CGPreflightPostEventAccess());
}

bool ActivatePid(pid_t pid) {
  if (pid <= 0) return false;
  NSRunningApplication* application =
      [NSRunningApplication runningApplicationWithProcessIdentifier:pid];
  if (application == nil) return false;

#pragma clang diagnostic push
#pragma clang diagnostic ignored "-Wdeprecated-declarations"
  return [application activateWithOptions:NSApplicationActivateIgnoringOtherApps];
#pragma clang diagnostic pop
}

/** Brings the previous app forward without pressing anything. */
napi_value Activate(napi_env env, napi_callback_info info) {
  pid_t pid = 0;
  if (!ReadPid(env, info, &pid)) return BoolValue(env, false);
  @autoreleasepool {
    return BoolValue(env, ActivatePid(pid));
  }
}

/**
 * Activates the target (if needed) and posts ⌘V.
 *
 * The key code is kVK_ANSI_V (9). Flags mirror Maccy: the command mask plus
 * 0x8 (the device-dependent left-Command bit) so apps that track modifier key
 * sides see the same event as a physical left-Command press.
 */
napi_value Paste(napi_env env, napi_callback_info info) {
  pid_t pid = 0;
  if (!ReadPid(env, info, &pid)) return BoolValue(env, false);

  if (!AXIsProcessTrustedWithOptions(nullptr)) return BoolValue(env, false);

  @autoreleasepool {
    if (pid > 0) {
      NSRunningApplication* target =
          [NSRunningApplication runningApplicationWithProcessIdentifier:pid];
      if (target != nil && ![target isActive]) {
        ActivatePid(pid);
        // NSRunningApplication reports activation asynchronously; give the
        // window server a short window to move focus before posting ⌘V.
        NSDate* deadline = [NSDate dateWithTimeIntervalSinceNow:0.25];
        while (![target isActive] && [deadline timeIntervalSinceNow] > 0) {
          [NSThread sleepForTimeInterval:0.002];
        }
        // Do not paste into whatever happens to be frontmost when the target
        // never came forward; the caller falls back to AppleScript instead.
        if (![target isActive]) return BoolValue(env, false);
      }
    }

    CGEventSourceRef source = CGEventSourceCreate(kCGEventSourceStateCombinedSessionState);
    if (source != nullptr) {
      // Swallow the Return / Option key-up of the launcher while posting, the
      // same way Maccy prevents the still-held keys from polluting ⌘V.
      CGEventSourceSetLocalEventsFilterDuringSuppressionState(
          source,
          kCGEventFilterMaskPermitLocalMouseEvents | kCGEventFilterMaskPermitSystemDefinedEvents,
          kCGEventSuppressionStateSuppressionInterval);
    }

    const CGEventFlags flags =
        static_cast<CGEventFlags>(kCGEventFlagMaskCommand | 0x000008);
    CGEventRef down = CGEventCreateKeyboardEvent(source, static_cast<CGKeyCode>(9), true);
    CGEventRef up = CGEventCreateKeyboardEvent(source, static_cast<CGKeyCode>(9), false);
    bool ok = down != nullptr && up != nullptr;
    if (ok) {
      CGEventSetFlags(down, flags);
      CGEventSetFlags(up, flags);
      CGEventPost(kCGSessionEventTap, down);
      CGEventPost(kCGSessionEventTap, up);
    }
    if (down != nullptr) CFRelease(down);
    if (up != nullptr) CFRelease(up);
    if (source != nullptr) CFRelease(source);
    return BoolValue(env, ok);
  }
}

}  // namespace

NAPI_MODULE_INIT(/* napi_env env, napi_value exports */) {
  napi_value frontmost = nullptr;
  napi_value activate = nullptr;
  napi_value paste = nullptr;
  napi_value bundleId = nullptr;
  napi_value accessibilityTrusted = nullptr;
  napi_value postEventTrusted = nullptr;
  napi_create_function(env, "frontmost", NAPI_AUTO_LENGTH, Frontmost, nullptr, &frontmost);
  napi_create_function(env, "activate", NAPI_AUTO_LENGTH, Activate, nullptr, &activate);
  napi_create_function(env, "paste", NAPI_AUTO_LENGTH, Paste, nullptr, &paste);
  napi_create_function(env, "bundleId", NAPI_AUTO_LENGTH, BundleId, nullptr, &bundleId);
  napi_create_function(env, "accessibilityTrusted", NAPI_AUTO_LENGTH, AccessibilityTrusted,
                       nullptr, &accessibilityTrusted);
  napi_create_function(env, "postEventTrusted", NAPI_AUTO_LENGTH, PostEventTrusted, nullptr,
                       &postEventTrusted);
  napi_set_named_property(env, exports, "frontmost", frontmost);
  napi_set_named_property(env, exports, "activate", activate);
  napi_set_named_property(env, exports, "paste", paste);
  napi_set_named_property(env, exports, "bundleId", bundleId);
  napi_set_named_property(env, exports, "accessibilityTrusted", accessibilityTrusted);
  napi_set_named_property(env, exports, "postEventTrusted", postEventTrusted);
  return exports;
}
