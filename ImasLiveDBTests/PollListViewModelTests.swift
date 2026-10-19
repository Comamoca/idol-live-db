import XCTest
@testable import ImasLiveDB

@MainActor
final class PollListViewModelTests: XCTestCase {

    private func makePoll(id: String, active: Bool) -> Poll {
        Poll(id: id, title: "お題\(id)", description: nil, targetType: .song,
             createdBy: "u1", createdAt: Date(),
             endsAt: Date().addingTimeInterval(active ? 86400 : -86400),
             status: "active", totalVotes: 0, entryCount: 0,
             candidateScope: .all, scopeBrandIds: nil, scopeEntityIds: nil, topEntityId: nil)
    }

    func testLoadActivePopulatesActiveList() async {
        let fake = FakeCommunityVoting()
        fake.pollsByStatus["active"] = [makePoll(id: "p1", active: true)]
        let vm = PollListViewModel(voting: fake)

        await vm.load(active: true)

        XCTAssertFalse(vm.isLoading)
        XCTAssertNil(vm.loadError)
        XCTAssertEqual(vm.polls(active: true).map(\.id), ["p1"])
        XCTAssertTrue(vm.polls(active: false).isEmpty)
    }

    func testLoadErrorSetsMessage() async {
        let fake = FakeCommunityVoting()
        fake.shouldThrow = true
        let vm = PollListViewModel(voting: fake)

        await vm.load(active: true)

        XCTAssertNotNil(vm.loadError)
    }

    func testPastLoadsNewestMonthThenGoesBackOneMonthAtATime() async {
        let fake = FakeCommunityVoting()
        fake.monthsToReturn = [
            PollMonth(month: "2026-10", label: "2026.10", count: 1),
            PollMonth(month: "2026-09", label: "2026.09", count: 2),
        ]
        fake.pastPollsByMonth = [
            "2026-10": [makePoll(id: "oct", active: false)],
            "2026-09": [makePoll(id: "sep1", active: false), makePoll(id: "sep2", active: false)],
        ]
        let vm = PollListViewModel(voting: fake)

        await vm.load(active: false)
        XCTAssertEqual(vm.polls(active: false).map(\.id), ["oct"])
        XCTAssertTrue(vm.hasMorePast)

        await vm.loadMorePast()
        XCTAssertEqual(vm.pastSections.map(\.id), ["2026-10", "2026-09"])
        XCTAssertEqual(vm.polls(active: false).map(\.id), ["oct", "sep1", "sep2"])
        XCTAssertFalse(vm.hasMorePast)

        // 読み直しても、さかのぼった月は畳まない。
        await vm.load(active: false)
        XCTAssertEqual(vm.pastSections.map(\.id), ["2026-10", "2026-09"])
        XCTAssertEqual(fake.pastPollMonthCalls, ["2026-10", "2026-09", "2026-10", "2026-09"])
    }

    func testInsertCreatedPrependsActivePoll() async {
        let fake = FakeCommunityVoting()
        fake.pollsByStatus["active"] = [makePoll(id: "p1", active: true)]
        let vm = PollListViewModel(voting: fake)
        await vm.load(active: true)

        vm.insertCreated(makePoll(id: "new", active: true))

        XCTAssertEqual(vm.polls(active: true).map(\.id), ["new", "p1"])
    }
}

@MainActor
final class PollHallOfFameViewModelTests: XCTestCase {

    func testLoadPopulatesResults() async {
        let fake = FakeCommunityVoting()
        fake.resultsToReturn = [
            PollResult(pollId: "p1", title: "最強の曲", targetType: .song,
                       endsAt: Date(), entityId: "s1", voteCount: 42)
        ]
        let vm = PollHallOfFameViewModel(voting: fake)

        await vm.load()

        XCTAssertFalse(vm.isLoading)
        XCTAssertNil(vm.loadError)
        XCTAssertEqual(vm.results.map(\.entityId), ["s1"])
    }

    func testLoadErrorSetsMessage() async {
        let fake = FakeCommunityVoting()
        fake.shouldThrow = true
        let vm = PollHallOfFameViewModel(voting: fake)

        await vm.load()

        XCTAssertNotNil(vm.loadError)
        XCTAssertTrue(vm.results.isEmpty)
    }
}
