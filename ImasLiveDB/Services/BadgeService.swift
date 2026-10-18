import Foundation
import Observation

/// 自分の貢献バッジ (`GET /users/me/badges`)。マイ投稿の件数と内訳の出どころ。
/// 件数はサーバ (D1) にだけあり、端末には持たない。
@MainActor
@Observable
final class BadgeService {
    static let shared = BadgeService()

    private var cache: (badges: UserBadges, fetchedAt: Date)?
    private let cacheDuration: TimeInterval = 300 // 5 minutes

    private init() {}

    /// 自分のバッジ。未ログイン・取れなかったときは nil。
    func currentUserBadges(forceReload: Bool = false) async -> UserBadges? {
        guard AuthService.shared.isSignedIn else { return nil }
        if !forceReload, let cache, Date().timeIntervalSince(cache.fetchedAt) < cacheDuration {
            return cache.badges
        }
        do {
            let badges: UserBadges = try await APIClient.shared.request(
                "GET", path: "/users/me/badges", authorized: true)
            cache = (badges, Date())
            return badges
        } catch {
            return nil
        }
    }

    /// サインアウト・アカウント切替で前のアカウントの値を捨てる。
    func invalidateAll() {
        cache = nil
    }
}
