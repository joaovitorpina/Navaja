// Spike S2.3 (docs/spikes.md): what the macOS half of the tray check needs
// beyond `screencapture`. s2-3-check.sh compiles it once with swiftc and runs
// it between captures; it never changes anything on the machine.
//
// Usage (rectangles are in points, as the window server reports them; the
// helper scales them to the capture's pixels):
//   s2-3-menubar screen                     # the main display: points, pixels, scale, menu bar
//   s2-3-menubar items                      # every on-screen window in the menu bar, left to right
//   s2-3-menubar windows                    # every on-screen window, for when `items` finds none
//   s2-3-menubar wait-item <count> <x> <seconds>  # waits for one new status item, left of <x>; prints it
//   s2-3-menubar wait-gone <x> <y> <w> <h> <seconds>  # waits for the status item there to go
//   s2-3-menubar measure <png> <x> <y> <w> <h>        # what the capture shows there (see Measure)
//   s2-3-menubar crop <png> <out.png> <x> <y> <w> <h> [zoom]  # a crop, enlarged without smoothing
//   s2-3-menubar present <png> <x> <y> <w> <h>        # PASS when an icon is drawn there
//   s2-3-menubar tint <light.png> <dark.png> <icon x y w h> <reference x y w h>
//                                           # PASS when the icon is monochrome and takes the
//                                           # reference item's colour in both appearances
import AppKit
import Foundation
import ImageIO
import UniformTypeIdentifiers

func fail(_ message: String) -> Never {
  print("::error::S2.3 \(message)")
  exit(1)
}

func double(_ text: String) -> Double {
  guard let value = Double(text) else { fail("not a number: \(text)") }
  return value
}

/// The main display in points, as the window server lays windows out.
func displayBounds() -> CGRect {
  CGDisplayBounds(CGMainDisplayID())
}

/// NSStatusBar's thickness, in points. On macOS 26 the bar is drawn taller
/// than this (its status item windows are), so the strip below allows more.
func statusBarThickness() -> CGFloat {
  NSStatusBar.system.thickness
}

struct Window {
  let owner: String
  let pid: Int32
  let layer: Int
  let bounds: CGRect
}

/// On-screen windows at the top of the display, no taller than a menu bar
/// can be (50 points) and narrower than 400, left to right.
func menuBarWindows() -> [Window] {
  allWindows().filter {
    $0.bounds.minY >= 0 && $0.bounds.minY <= 1 && $0.bounds.height <= 50 && $0.bounds.width < 400
      && $0.bounds.width > 0
  }
  .sorted { $0.bounds.minX < $1.bounds.minX }
}

/// Every on-screen window, front to back.
func allWindows() -> [Window] {
  let options: CGWindowListOption = [.optionOnScreenOnly, .excludeDesktopElements]
  guard let list = CGWindowListCopyWindowInfo(options, kCGNullWindowID) as? [[String: Any]] else {
    fail("CGWindowListCopyWindowInfo returned nothing")
  }
  return list.compactMap { info -> Window? in
    guard let layer = info[kCGWindowLayer as String] as? Int,
      let dictionary = info[kCGWindowBounds as String] as? NSDictionary,
      let bounds = CGRect(dictionaryRepresentation: dictionary as CFDictionary)
    else { return nil }
    let owner = info[kCGWindowOwnerName as String] as? String ?? "?"
    let pid = (info[kCGWindowOwnerPID as String] as? Int).map { Int32($0) } ?? -1
    return Window(owner: owner, pid: pid, layer: layer, bounds: bounds)
  }
}

/// The status level, where NSStatusItem windows live.
let statusLevel = Int(CGWindowLevelForKey(.statusWindow))

/// The menu bar's status items, left to right.
func statusItems() -> [Window] {
  menuBarWindows().filter { $0.layer == statusLevel }
}

func describe(_ rect: CGRect) -> String {
  "\(fmt(rect.minX)) \(fmt(rect.minY)) \(fmt(rect.width)) \(fmt(rect.height))"
}

func fmt(_ value: Double, _ digits: Int = 1) -> String {
  String(format: "%.\(digits)f", value)
}

/// A capture as 8-bit sRGB pixels, top row first.
struct Capture {
  let width: Int
  let height: Int
  let pixels: [UInt8]
  /// Capture pixels per point.
  let scale: Double

  init(path: String) {
    let url = URL(fileURLWithPath: path)
    guard let source = CGImageSourceCreateWithURL(url as CFURL, nil),
      let image = CGImageSourceCreateImageAtIndex(source, 0, nil)
    else { fail("cannot read \(path)") }
    // Locals: the closure below may not capture self before it is whole.
    let w = image.width
    let h = image.height
    var buffer = [UInt8](repeating: 0, count: w * h * 4)
    let drawn = buffer.withUnsafeMutableBytes { bytes -> Bool in
      guard
        let context = CGContext(
          data: bytes.baseAddress, width: w, height: h, bitsPerComponent: 8,
          bytesPerRow: w * 4, space: CGColorSpace(name: CGColorSpace.sRGB)!,
          bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)
      else { return false }
      context.draw(image, in: CGRect(x: 0, y: 0, width: w, height: h))
      return true
    }
    guard drawn else { fail("cannot decode \(path)") }
    width = w
    height = h
    pixels = buffer
    scale = Double(w) / Double(displayBounds().width)
  }

  /// `rect`, in points, as whole capture pixels inside the image.
  func pixelRect(_ rect: CGRect) -> (x0: Int, y0: Int, x1: Int, y1: Int) {
    let x0 = max(0, Int((rect.minX * scale).rounded()))
    let y0 = max(0, Int((rect.minY * scale).rounded()))
    let x1 = min(width, Int((rect.maxX * scale).rounded()))
    let y1 = min(height, Int((rect.maxY * scale).rounded()))
    guard x1 > x0 + 2, y1 > y0 + 2 else { fail("rectangle \(describe(rect)) is outside the capture") }
    return (x0, y0, x1, y1)
  }

  func rgb(_ x: Int, _ y: Int) -> (Double, Double, Double) {
    let i = (y * width + x) * 4
    return (Double(pixels[i]), Double(pixels[i + 1]), Double(pixels[i + 2]))
  }
}

func luma(_ c: (Double, Double, Double)) -> Double {
  0.2126 * c.0 + 0.7152 * c.1 + 0.0722 * c.2
}

func median(_ values: [Double]) -> Double {
  let sorted = values.sorted()
  return sorted[sorted.count / 2]
}

/// What one rectangle of a capture shows. The background is the median of
/// the rectangle's outermost ring of pixels, which a menu bar item leaves
/// empty. Ink is every pixel at least `inkContrast` away from it in some
/// channel; the core is the ink at 60 % or more of the strongest contrast,
/// the glyph without its anti-aliased edge.
struct Measure {
  static let inkContrast = 48.0

  let pixels: Int
  let background: (Double, Double, Double)
  let ink: Int
  let core: Int
  let coreColor: (Double, Double, Double)
  /// Mean of max - min over the core's channels: 0 for grey.
  let coreSaturation: Double
  let maxContrast: Double
  /// The ink's bounding box, in capture pixels: how large the glyph is drawn.
  let inkBox: (width: Int, height: Int)

  init(_ capture: Capture, _ rect: CGRect) {
    let r = capture.pixelRect(rect)
    var ring: [(Double, Double, Double)] = []
    for x in r.x0..<r.x1 {
      ring.append(capture.rgb(x, r.y0))
      ring.append(capture.rgb(x, r.y1 - 1))
    }
    for y in r.y0..<r.y1 {
      ring.append(capture.rgb(r.x0, y))
      ring.append(capture.rgb(r.x1 - 1, y))
    }
    let bg = (median(ring.map { $0.0 }), median(ring.map { $0.1 }), median(ring.map { $0.2 }))
    var contrasts: [(Double, (Double, Double, Double))] = []
    var box = (x0: Int.max, y0: Int.max, x1: Int.min, y1: Int.min)
    for y in r.y0..<r.y1 {
      for x in r.x0..<r.x1 {
        let c = capture.rgb(x, y)
        let d = max(abs(c.0 - bg.0), abs(c.1 - bg.1), abs(c.2 - bg.2))
        contrasts.append((d, c))
        if d >= Measure.inkContrast {
          box = (min(box.x0, x), min(box.y0, y), max(box.x1, x), max(box.y1, y))
        }
      }
    }
    inkBox = box.x1 >= box.x0 ? (box.x1 - box.x0 + 1, box.y1 - box.y0 + 1) : (0, 0)
    let strongest = contrasts.map { $0.0 }.max() ?? 0
    let inkPixels = contrasts.filter { $0.0 >= Measure.inkContrast }
    let corePixels = inkPixels.filter { $0.0 >= 0.6 * strongest }
    let n = Double(max(1, corePixels.count))
    pixels = contrasts.count
    background = bg
    ink = inkPixels.count
    core = corePixels.count
    coreColor = (
      corePixels.map { $0.1.0 }.reduce(0, +) / n,
      corePixels.map { $0.1.1 }.reduce(0, +) / n,
      corePixels.map { $0.1.2 }.reduce(0, +) / n
    )
    coreSaturation =
      corePixels.map { max($0.1.0, $0.1.1, $0.1.2) - min($0.1.0, $0.1.1, $0.1.2) }.reduce(0, +) / n
    maxContrast = strongest
  }

  var inkShare: Double { Double(ink) / Double(max(1, pixels)) }

  var summary: String {
    "pixels=\(pixels) background=\(color(background)) (luma \(fmt(luma(background), 0))) "
      + "ink=\(ink) (\(fmt(inkShare * 100))%) core=\(core) core_colour=\(color(coreColor)) "
      + "(luma \(fmt(luma(coreColor), 0)), saturation \(fmt(coreSaturation))) "
      + "max_contrast=\(fmt(maxContrast, 0)) ink_box=\(inkBox.width)x\(inkBox.height)"
  }
}

func color(_ c: (Double, Double, Double)) -> String {
  String(format: "#%02x%02x%02x", Int(c.0.rounded()), Int(c.1.rounded()), Int(c.2.rounded()))
}

func rect(_ args: ArraySlice<String>) -> CGRect {
  let values = args.map(double)
  guard values.count == 4 else { fail("a rectangle is four numbers: x y w h") }
  return CGRect(x: values[0], y: values[1], width: values[2], height: values[3])
}

func writePNG(_ image: CGImage, to path: String) {
  let url = URL(fileURLWithPath: path) as CFURL
  guard let destination = CGImageDestinationCreateWithURL(url, UTType.png.identifier as CFString, 1, nil)
  else { fail("cannot write \(path)") }
  CGImageDestinationAddImage(destination, image, nil)
  guard CGImageDestinationFinalize(destination) else { fail("cannot write \(path)") }
}

/// An icon is drawn in `rect` when at least 3 % of its pixels are ink and
/// the strongest contrast is at least 96 of 255. An 18-point template glyph
/// in its status item covers well over that; an empty stretch of menu bar
/// has no ink at all.
func present(_ measure: Measure) -> Bool {
  measure.inkShare >= 0.03 && measure.maxContrast >= 96
}

let args = CommandLine.arguments
switch args.count > 1 ? args[1] : "" {
case "screen":
  let bounds = displayBounds()
  let mode = CGDisplayCopyDisplayMode(CGMainDisplayID())
  let pixelsWide = mode?.pixelWidth ?? 0
  let pixelsHigh = mode?.pixelHeight ?? 0
  let scale = NSScreen.main?.backingScaleFactor ?? 0
  let itemHeight = statusItems().map { $0.bounds.maxY }.max() ?? 0
  print(
    "display: \(fmt(bounds.width, 0))x\(fmt(bounds.height, 0)) points, "
      + "\(pixelsWide)x\(pixelsHigh) pixels, backing scale \(fmt(scale)); "
      + "NSStatusBar thickness \(fmt(statusBarThickness())) points; status items "
      + "\(fmt(itemHeight)) points tall; status level \(statusLevel)")

case "windows":
  for window in allWindows() {
    print(
      "\(describe(window.bounds))  layer \(window.layer)  pid \(window.pid)  \(window.owner)")
  }

case "items":
  for window in menuBarWindows() {
    print(
      "\(describe(window.bounds))  layer \(window.layer)  pid \(window.pid)  \(window.owner)")
  }

case "wait-item":
  // On macOS 26 Control Center owns every status item window, the app's
  // too, so the owner cannot tell them apart. A new item goes to the left
  // of the others: the app's is the one that makes the count one higher
  // and sits left of the leftmost item before it started.
  guard args.count == 5, let before = Int(args[2]) else {
    fail("usage: wait-item <items before> <leftmost x before> <seconds>")
  }
  let leftmost = double(args[3])
  let deadline = Date().addingTimeInterval(double(args[4]))
  // The item may change size once its image is set: wait until the new
  // item's rectangle has held for a second.
  var seen: CGRect? = nil
  var since = Date()
  while true {
    let items = statusItems()
    if items.count > before + 1 {
      fail("\(items.count - before) new status items, not one: \(items.map { describe($0.bounds) })")
    }
    if items.count == before + 1, let first = items.first, Double(first.bounds.minX) < leftmost {
      if first.bounds != seen {
        seen = first.bounds
        since = Date()
      } else if Date().timeIntervalSince(since) >= 1 {
        print(describe(first.bounds))
        exit(0)
      }
    } else {
      seen = nil
    }
    if Date() > deadline { fail("no new status item in the menu bar within \(args[4]) s") }
    Thread.sleep(forTimeInterval: 0.1)
  }

case "wait-gone":
  guard args.count == 7 else { fail("usage: wait-gone <x> <y> <w> <h> <seconds>") }
  let gone = rect(args[2...5])
  let deadline = Date().addingTimeInterval(double(args[6]))
  while statusItems().contains(where: { $0.bounds == gone }) {
    if Date() > deadline { fail("the status item at \(describe(gone)) is still there after \(args[6]) s") }
    Thread.sleep(forTimeInterval: 0.25)
  }
  print("The status item at \(describe(gone)) is gone.")

case "measure":
  guard args.count == 7 else { fail("usage: measure <png> <x> <y> <w> <h>") }
  print(Measure(Capture(path: args[2]), rect(args[3...6])).summary)

case "crop":
  guard args.count == 8 || args.count == 9 else { fail("usage: crop <png> <out> <x> <y> <w> <h> [zoom]") }
  let capture = Capture(path: args[2])
  let r = capture.pixelRect(rect(args[4...7]))
  let zoom = args.count == 9 ? Int(double(args[8])) : 1
  let w = (r.x1 - r.x0) * zoom
  let h = (r.y1 - r.y0) * zoom
  var out = [UInt8](repeating: 255, count: w * h * 4)
  for y in 0..<h {
    for x in 0..<w {
      let src = ((r.y0 + y / zoom) * capture.width + r.x0 + x / zoom) * 4
      let dst = (y * w + x) * 4
      out[dst] = capture.pixels[src]
      out[dst + 1] = capture.pixels[src + 1]
      out[dst + 2] = capture.pixels[src + 2]
      out[dst + 3] = 255
    }
  }
  let cropped = out.withUnsafeMutableBytes { bytes -> CGImage? in
    CGContext(
      data: bytes.baseAddress, width: w, height: h, bitsPerComponent: 8, bytesPerRow: w * 4,
      space: CGColorSpace(name: CGColorSpace.sRGB)!,
      bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)?.makeImage()
  }
  guard let cropped else { fail("cannot crop \(args[2])") }
  writePNG(cropped, to: args[3])
  print("\(args[3]): \(w)x\(h) pixels (\(r.x1 - r.x0)x\(r.y1 - r.y0) from the capture, x\(zoom))")

case "present":
  guard args.count == 7 else { fail("usage: present <png> <x> <y> <w> <h>") }
  let measure = Measure(Capture(path: args[2]), rect(args[3...6]))
  print("\(args[2]): \(measure.summary)")
  guard present(measure) else {
    fail("no icon in \(describe(rect(args[3...6]))) of \(args[2]): ink \(fmt(measure.inkShare * 100))% (needs 3%), max contrast \(fmt(measure.maxContrast, 0)) (needs 96)")
  }
  print("An icon is drawn there.")

case "tint":
  guard args.count == 12 else {
    fail("usage: tint <light.png> <dark.png> <icon x y w h> <reference x y w h>")
  }
  let icon = rect(args[4...7])
  let reference = rect(args[8...11])
  var problems: [String] = []
  var iconLuma: [String: Double] = [:]
  for (name, path) in [("light", args[2]), ("dark", args[3])] {
    let capture = Capture(path: path)
    let mine = Measure(capture, icon)
    let theirs = Measure(capture, reference)
    print("\(name): icon      \(mine.summary)")
    print("\(name): reference \(theirs.summary)")
    if !present(mine) { problems.append("\(name): no icon drawn") }
    if !present(theirs) { problems.append("\(name): the reference item shows nothing") }
    // Monochrome: the core's channels stay within 24 of each other on
    // average, or within 12 more than the reference's, which the menu bar's
    // vibrancy may tint too.
    let limit = max(24, theirs.coreSaturation + 12)
    if mine.coreSaturation > limit {
      problems.append("\(name): the icon is not monochrome (saturation \(fmt(mine.coreSaturation)), limit \(fmt(limit)))")
    }
    // The system's colour: the icon's core is as light as the reference
    // item's, within 48 of 255, and on the same side of the background.
    let difference = abs(luma(mine.coreColor) - luma(theirs.coreColor))
    if difference > 48 {
      problems.append("\(name): the icon's luma \(fmt(luma(mine.coreColor), 0)) is \(fmt(difference, 0)) from the reference's \(fmt(luma(theirs.coreColor), 0)) (limit 48)")
    }
    let mineSide = luma(mine.coreColor) > luma(mine.background)
    let theirSide = luma(theirs.coreColor) > luma(theirs.background)
    if mineSide != theirSide {
      problems.append("\(name): the icon is \(mineSide ? "lighter" : "darker") than the menu bar, the reference \(theirSide ? "lighter" : "darker")")
    }
    iconLuma[name] = luma(mine.coreColor)
  }
  // Tinted per appearance: the icon is dark in one and light in the other.
  if let light = iconLuma["light"], let dark = iconLuma["dark"], abs(dark - light) < 100 {
    problems.append("the icon's luma changes by \(fmt(abs(dark - light), 0)) between the appearances (needs 100)")
  }
  guard problems.isEmpty else { fail("tint: " + problems.joined(separator: "; ")) }
  print("The icon is monochrome and takes the reference item's colour in both appearances.")

default:
  fail("usage: s2-3-menubar screen|items|windows|wait-item|wait-gone|measure|crop|present|tint ...")
}
