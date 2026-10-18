# The items file: the game's rule tables

`data/raw/game_files/rules/items/items_v787.03.json` (19 MB, version 787.03, built 07/10/2026) is the game's own
rule data, downloaded by the client at login. It is one JSON object with **233 sections**, each a list of
rows (a few are objects). This is where prices, levels, defenders, timings and rewards come from; the server
sends only the live state on top of them. `ItemsVersion.properties` next to it holds the version
(`CastleItemXMLVersion=787.03`).

Values are mostly **strings** (`"kingdomTravellingCost": "250"`), and a row lists only the fields it uses.

## Sections the code uses today

| Section | Rows | What it gives | Used by |
| --- | ---: | --- | --- |
| `units` | 1001 | Every unit and tool: `wodID`, `type`, costs, speed, attack/defence, **`kingdomTravellingCost`**, `eventIDs`. Troops are `group: "Unit"` | `utils/scripts/build_tables.py` makes `data/tables/units.csv` and `generated/units.rs` from it |
| `kingdoms` | 6 | Per kingdom: `unitTravelTime`, `resourceTravelTime`, **`unitTravelTaxRate`**, `minLevel`, `skipUnitTravelC2Cost` | the same table; [`../game_infrastructure/kingdom_transfer_rules.md`](../game_infrastructure/kingdom_transfer_rules.md) |
| `currencyMinutesSkipValues` | 7 | The seven minute-skip items: currency id 1001 to 1007, minutes 1, 5, 10, 30, 60, 300, 1440 | the `MS1`..`MS7` keys ([`../network/msk.md`](../network/msk.md)) |
| `dungeons` | 1769 | Robber baron **defenders** per victory count and kingdom (`countVictories`, `kID`, `lordID`, `unitsL/M/R/K`, tools, `skipCosts`). It does **not** give levels | [`../game_infrastructure/robber_baron_level_formula.md`](../game_infrastructure/robber_baron_level_formula.md) |
| `bossdungeons`, `nomadCamps`, `samuraiCamps`, `factioninvasioncamps`, `allianceBattleGroundDungeons` | 3, 90, 90, 154, 40 | Other NPC camp types, each with its own `dungeonlevel` | not used yet |
| `buildings`, `buildingPositions` | 4032, 1820 | Building definitions and where they sit | not used yet |
| `levelups`, `levelUpPrices`, `viplevels`, `gemlevels` | 10, 60, 10, 14 | Player level and VIP tables | not used yet |

Everything else is in the lists below and has **not been examined**. A section is added to this table the first
time something depends on it.

## All sections, grouped by name (not by examination)

Grouping is by the section's name only. The number is the row count.

* **alliance**: allianceBattleGroundDungeons(40), allianceBattleGroundMaps(4), allianceBattleGroundPreBuiltCastles(4), allianceBattleGroundRankRewards(68), allianceBattleGroundScorings(2), allianceBattleGroundSettings(15), allianceCoatColors(20), allianceCoatLayouts(19), allianceForges(30), allianceFundsDonatables(17), allianceInvasionCamps(26), allianceQuests(170), allianceTowerEffects(5), allianceTowerEffectsActivations(6), allianceTowers(5), alliancebuffs(261), alliancefameranks(50), alliancehelprequests(6), alliancerankrights(19), allianceranks(10)
* **construction**: constructionItemDisassemblingTombolas(7), constructionItemRecipes(788), constructionItems(2169), constructionItemsEffectGroups(51)
* **crafting**: craftingQueues(4), craftingRecipes(507)
* **currency**: currencyBoosters(4), currencyCaps(12), currencyClearCompensations(148), currencyCollectTaskTypes(7), currencyHiddenLists(4), currencyMinutesSkipValues(7), currencyRandomBonuses(7), currencyRarenesses(25), currencySpendTaskTypes(3), currencyTypes(12)
* **daimyo**: daimyoCastleAllianceContracts(40), daimyoCastles(30), daimyoEndRewards(168), daimyoTownshipAllianceContracts(40), daimyoTownships(30)
* **donation**: donationItems(195), donationRewards(260), donationSettings(13), donationTypes(3)
* **equipment**: equipment_drops(312), equipment_effects(338), equipment_effectstrengths(409), equipment_groups(13), equipment_rarenesses(10), equipment_sets(678), equipment_slots(6), equipment_wearers(2), equipments(1577)
* **event**: eventSkins(7), eventannouncements(21), events(123)
* **eventAutoScaling**: eventAutoScalingCIs(4), eventAutoScalingCamps(1078), eventAutoScalingDifficulties(44), eventAutoScalingDifficultyTypes(11), eventAutoScalingHoLSkills(14), eventAutoScalingLordEffects(528), eventAutoScalingToolPairings(1303), eventAutoScalingUnitPairings(2518), eventAutoScalings(44)
* **fusion**: fusionCostSequences(4), fusionForges(1), fusionShops(2), fusionSystems(1)
* **gem**: gemColors(9), gemlevels(14), gems(501)
* **general**: generalAbilities(87), generalAbilityEffects(165), generalAbilityTriggers(4), generalRarities(4), generalSkillTiers(77), generalSkills(2368), generalSkipFinishQuests(7), generalSlots(104), generalXpItems(9), generals(37), generalsRespecPackages(1)
* **leaguetype**: leaguetypeevents(377), leaguetypes(89)
* **lootBox**: lootBoxKeyTombolas(24), lootBoxTombolas(6381), lootBoxTypes(1), lootBoxes(46)
* **luckywheel**: luckywheelclasses(9), luckywheelrewardsets(520)
* **officersSchool**: officersSchoolCurrencies(3), officersSchoolDurations(1), officersSchoolEffects(5), officersSchoolPowerDistributions(100), officersSchoolUnitPairs(14)
* **package**: packageCategories(34), packageCategoryCostRelations(23), packageCategoryFilterRelations(193), packageFilters(14), packageSubFilters(45), packages(6475)
* **payment**: paymentrewards(3218)
* **pointevent**: pointeventquests(170), pointeventrewardsets(63), pointeventtypes(28)
* **raidBoss**: raidBossLevels(60), raidBossStages(397), raidBosses(3)
* **relic**: relicBluePrints(38), relicCategories(7), relicEffectLists(36), relicEffectPowerRatings(7), relicEffects(285), relicEnchanters(50), relicLootOptions(220), relicPowerDistributions(100), relicTypes(19)
* **reward**: rewardBags(56), rewards(28489)
* **sagamap**: sagamaps(619)
* **saleDays**: saleDaysLuckyWheelClasses(9), saleDaysLuckyWheelInitialCategorySelections(5), saleDaysLuckyWheelRewardSets(180)
* **samurai**: samuraiCamps(90), samuraiCardCosts(18), samuraiLevelBrackets(6)
* **sceat**: sceatSkillTiers(24), sceatSkillTrees(6), sceatSkills(333)
* **season**: seasonEndRewards(192), seasonEventRewards(798), seasonMedals(7), seasonPromotionRewards(480), seasonRanks(21), seasonSettings(1)
* **tempServer**: tempServerDailyTaskRewards(5), tempServerPreBuiltCastles(5), tempServerRankPoints(47), tempServerRankRewards(412), tempServerSettings(16)
* **other**: !(3), achievements(597), activityrewards(11), alienRerollChances(50), artifacts(4), artifactsLeagues(12), attackSetupSlots(30), beginner_loginrewards(7), boostConsumables(3), bossdungeons(3), buildingPositions(1820), buildings(4032), catalysts(80), characters(3), collectorEventOptions(23), collectorEventRewards(128), crestsymbols(82), currencies(271), dailyactivities(22), dailyquests(19), dailyrewards(5), districtTypes(6), divisionRounds(12), dungeons(1769), effectCaps(278), effects(830), effecttypes(239), emptyAreas(24), expansions(137), externalServerSkins(1), factioninvasioncamps(154), feasts(10), forgeMinuteSkips(2), fortuneTellerClasses(14), gachaEvents(123), globalEffects(19), highscoreboni(30), horses(33), islandPlayerRewards(8), islandPreBuiltCastles(3), islandrewardranks(14), isles(14), judgements(5), kingdoms(6), kingsMarkets(2), laboratories(50), landmarks(4), leaderboardRewards(1140), legendskills(353), levelBoosters(73), levelUpPrices(60), levelups(10), longtermpointeventquests(20), lords(92), mainquests(8), messageRestrictions(3), mightranks(33), mineTypes(17), monuments(19), newsletterRewards(1), nomadCamps(90), partpayprices(142), playerNameChanges(18), prebuiltcastles(38), primeDays(2212), privateVillages(135), properties(8), quests(4664), randomgroups(26), rerollCosts(31), researches(1040), resources(13), shoppingCarts(1692), slums(16), specialcamps(91), startResources(9), subscriptionLoyalties(6), subscriptionsBuffs(94), subscriptionsRewards(1), titles(57), tmapnodes(499), tmaps(19), toolCategories(5), tutorials(34), units(1001), versionInfo(2), villages(3), viplevels(10), welcomeBackRewards(9), wishingwells(11), worldmapskins(154)

## Using it

* Read a section: `python3 -c "import json;d=json.load(open('data/raw/game_files/rules/items/items_v787.03.json'));print(d['kingdoms'])"`.
* A new game version means a new file name (`items_v<version>.json`). Capture again with `proxy_packet_monitor`, run
  `utils/scripts/organize_game_data.py`, then `utils/scripts/build_tables.py` to refresh the lookup tables and the Rust ([`tables.md`](tables.md)).
* To find what the client does with a section, search the client code for the field name ([`client.md`](client.md)).
