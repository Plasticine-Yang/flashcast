{
  "targets": [
    {
      "target_name": "paste",
      "sources": ["paste.mm"],
      "defines": ["NAPI_VERSION=8"],
      "xcode_settings": {
        "CLANG_ENABLE_OBJC_ARC": "YES",
        "CLANG_CXX_LANGUAGE_STANDARD": "c++17",
        "MACOSX_DEPLOYMENT_TARGET": "11.0",
        "OTHER_CFLAGS": ["-Wno-deprecated-declarations"]
      },
      "link_settings": {
        "libraries": ["-framework AppKit", "-framework ApplicationServices"]
      }
    }
  ]
}
