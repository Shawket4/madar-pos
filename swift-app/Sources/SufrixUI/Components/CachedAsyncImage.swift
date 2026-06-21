// Disk + memory image cache — parity with the Flutter teller app's
// `cached_network_image`: fast repeat loads AND offline display. Bytes are stored
// under Caches/menu-images keyed by a hash of the URL, ignoring server cache
// headers (menu photos rarely change and must survive offline). Apple's built-in
// `AsyncImage` only uses URLSession's volatile cache, which doesn't hold offline.
import SwiftUI

#if canImport(UIKit)
import UIKit
typealias PlatformImage = UIImage
#elseif canImport(AppKit)
import AppKit
typealias PlatformImage = NSImage
#endif

final class ImageStore: @unchecked Sendable {
    static let shared = ImageStore()

    private let memory = NSCache<NSString, PlatformImage>()
    private let dir: URL
    private let session: URLSession

    private init() {
        let caches = FileManager.default.urls(for: .cachesDirectory, in: .userDomainMask)[0]
        dir = caches.appendingPathComponent("menu-images", isDirectory: true)
        try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        let cfg = URLSessionConfiguration.default
        cfg.timeoutIntervalForRequest = 15
        session = URLSession(configuration: cfg)
        memory.countLimit = 250
    }

    /// FNV-1a of the absolute URL → a stable on-disk filename.
    private func key(_ url: URL) -> String {
        var h: UInt64 = 1469598103934665603
        for b in url.absoluteString.utf8 { h = (h ^ UInt64(b)) &* 1099511628211 }
        return String(h, radix: 16)
    }

    /// A synchronous hit from memory or disk (nil = needs fetching).
    func cached(_ url: URL) -> PlatformImage? {
        let k = key(url) as NSString
        if let img = memory.object(forKey: k) { return img }
        let file = dir.appendingPathComponent(k as String)
        if let data = try? Data(contentsOf: file), let img = PlatformImage(data: data) {
            memory.setObject(img, forKey: k)
            return img
        }
        return nil
    }

    /// Fetch over the network, persisting to disk + memory. Returns nil on failure
    /// (offline with no cached copy) so the caller shows the gradient fallback.
    func load(_ url: URL) async -> PlatformImage? {
        if let img = cached(url) { return img }
        guard let (data, _) = try? await session.data(from: url),
              let img = PlatformImage(data: data) else { return nil }
        try? data.write(to: dir.appendingPathComponent(key(url)), options: .atomic)
        memory.setObject(img, forKey: key(url) as NSString)
        return img
    }
}

/// Drop-in cached async image. Renders the decoded photo when available and a
/// transparent `Color.clear` otherwise (so a gradient layered beneath shows).
struct CachedAsyncImage: View {
    let url: URL
    @State private var image: PlatformImage?

    var body: some View {
        Group {
            if let image {
                platformImage(image).resizable().scaledToFill()
            } else {
                Color.clear
            }
        }
        .task(id: url) {
            if let hit = ImageStore.shared.cached(url) { image = hit; return }
            image = await ImageStore.shared.load(url)
        }
    }

    private func platformImage(_ img: PlatformImage) -> Image {
        #if canImport(UIKit)
        Image(uiImage: img)
        #else
        Image(nsImage: img)
        #endif
    }
}
