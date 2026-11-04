// Writes the icon macOS itself shows for a bundle, at a given pixel size.
import AppKit
let a=CommandLine.arguments; let n=Int(a[3])!
let i=NSWorkspace.shared.icon(forFile:a[1])
let r=NSBitmapImageRep(bitmapDataPlanes:nil,pixelsWide:n,pixelsHigh:n,bitsPerSample:8,samplesPerPixel:4,hasAlpha:true,isPlanar:false,colorSpaceName:.deviceRGB,bytesPerRow:0,bitsPerPixel:0)!
NSGraphicsContext.saveGraphicsState();NSGraphicsContext.current=NSGraphicsContext(bitmapImageRep:r)
i.draw(in:NSRect(x:0,y:0,width:n,height:n));NSGraphicsContext.restoreGraphicsState()
try! r.representation(using:.png,properties:[:])!.write(to:URL(fileURLWithPath:a[2]))
