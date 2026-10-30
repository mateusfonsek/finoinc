// Which package the hero button offers. Reads only what the browser already says
// about itself: nothing is requested, nothing leaves the page.
(function (scope) {
  const BASE = "https://github.com/MateusFonseK/finoinc/releases/latest/download/";
  const PACKAGES = {
    mac: BASE + "Finoinc_universal.dmg",
    windows: BASE + "Finoinc_x64-setup.exe",
    linux: BASE + "Finoinc_amd64.AppImage",
  };

  /** "mac" | "windows" | "linux", or null when no desktop package fits. */
  function pickPackage(userAgent, platform, touchPoints) {
    const ua = String(userAgent);
    const where = `${ua} ${platform}`;
    // Phones and tablets first: Android says "Linux", iPhone says "like Mac OS X",
    // and neither runs a desktop app. ChromeOS says "Linux" too.
    if (/iPhone|iPad|iPod|Android|CrOS/i.test(ua)) return null;
    if (/Mac/i.test(where)) {
      // iPadOS asks for desktop sites with a Mac user agent; only touch gives it away.
      return touchPoints > 1 ? null : "mac";
    }
    if (/Win/i.test(where)) return "windows";
    // The AppImage is x86-64 only. An ARM platform is a Raspberry Pi, an ARM
    // laptop, or an Android phone asking for the desktop site.
    if (/arm|aarch64/i.test(platform)) return null;
    if (/Linux|X11/i.test(where)) return "linux";
    return null;
  }

  scope.finoincDownload = { PACKAGES, pickPackage };
})(window);
