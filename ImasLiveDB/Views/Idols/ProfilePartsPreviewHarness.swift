#if DEBUG
import SwiftUI

/// アイドルのプロフィール (見た目・話し方・呼び方)・呼び方の全員・ユニットの呼称表を
/// シミュレータで撮る DEBUG 専用ハーネス (`PRODUCER_CARD_PREVIEW` と同じ流儀)。
///
///     SIMCTL_CHILD_SCREENSHOT_MODE=1 SIMCTL_CHILD_PROFILE_PARTS_PREVIEW=profile \
///     SIMCTL_CHILD_PREVIEW_IDOL=三峰結華 SIMCTL_CHILD_PREVIEW_UNIT=アンティーカ \
///     xcrun simctl launch <udid> com.fugaif.ImasLiveDB
struct ProfilePartsPreviewHarness: View {
    enum Mode: String {
        /// アイドル詳細のプロフィールのタブ。
        case profile
        /// 呼び方の全員。
        case calls
        /// ユニット詳細の呼称表のタブ。
        case unit
    }

    static var envMode: Mode? {
        ProcessInfo.processInfo.environment["PROFILE_PARTS_PREVIEW"].flatMap(Mode.init(rawValue:))
    }

    let mode: Mode
    @State private var idol: Idol?
    @State private var unit: Unit?

    var body: some View {
        NavigationStack {
            switch mode {
            case .profile:
                if let idol { IdolDetailView(idol: idol, initialSegment: 2) } else { ImasLoadingState() }
            case .calls:
                if let idol { IdolCallListView(idol: idol) } else { ImasLoadingState() }
            case .unit:
                if let unit { UnitDetailView(unit: unit, initialTab: .calls) } else { ImasLoadingState() }
            }
        }
        .task {
            let env = ProcessInfo.processInfo.environment
            let idolName = env["PREVIEW_IDOL"] ?? "三峰結華"
            let unitName = env["PREVIEW_UNIT"] ?? "アンティーカ"
            idol = try? await AppContainer.shared.idolReading.searchIdols(query: idolName, limit: 5).first { $0.name == idolName }
            unit = try? await AppContainer.shared.unitReading.allUnits().first { $0.name == unitName }
        }
    }
}
#endif
