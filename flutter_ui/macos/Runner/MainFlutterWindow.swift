import Cocoa
import FlutterMacOS
// Multi-window (pop-out panels): the desktop_multi_window macOS implementation
// pulled in by `flutter pub get`. See windows/runner/flutter_window.cpp and
// linux/runner/my_application.cc for the sibling wiring on the other two
// platforms.
import desktop_multi_window

class MainFlutterWindow: NSWindow {
  override func awakeFromNib() {
    let flutterViewController = FlutterViewController()
    let windowFrame = self.frame
    self.contentViewController = flutterViewController
    self.setFrame(windowFrame, display: true)

    RegisterGeneratedPlugins(registry: flutterViewController)

    // Each popped-out panel is a second Flutter engine in THIS process.
    // Register the app's plugins on every sub-window engine as it is created,
    // so a popout has the same plugin surface (file_selector, etc.) as the
    // main window. The engine bridge itself is dart:ffi, not a plugin, so it
    // needs no registrant — the sub-window opens the same already-loaded
    // liblumit_bridge.dylib directly.
    FlutterMultiWindowPlugin.setOnWindowCreatedCallback { controller in
      RegisterGeneratedPlugins(registry: controller)
    }

    super.awakeFromNib()
  }
}
