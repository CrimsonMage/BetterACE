//! Official ACE `GameAction/GameActionType.cs` at 47edade3bd3f6044b676d4eb877c4965c7eda62b.
//! Identifiers alone do not establish payload implementation.

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct GameActionType(pub u32);

#[allow(non_upper_case_globals)]
impl GameActionType {
    pub const SetSingleCharacterOption: Self = Self(0x0005);
    pub const TargetedMeleeAttack: Self = Self(0x0008);
    pub const TargetedMissileAttack: Self = Self(0x000A);
    pub const SetAfkMode: Self = Self(0x000F);
    pub const SetAfkMessage: Self = Self(0x0010);
    pub const Talk: Self = Self(0x0015);
    pub const RemoveFriend: Self = Self(0x0017);
    pub const AddFriend: Self = Self(0x0018);
    pub const PutItemInContainer: Self = Self(0x0019);
    pub const GetAndWieldItem: Self = Self(0x001A);
    pub const DropItem: Self = Self(0x001B);
    pub const SwearAllegiance: Self = Self(0x001D);
    pub const BreakAllegiance: Self = Self(0x001E);
    pub const AllegianceUpdateRequest: Self = Self(0x001F);
    pub const RemoveAllFriends: Self = Self(0x0025);
    pub const TeleToPklArena: Self = Self(0x0026);
    pub const TeleToPkArena: Self = Self(0x0027);
    pub const TitleSet: Self = Self(0x002C);
    pub const QueryAllegianceName: Self = Self(0x0030);
    pub const ClearAllegianceName: Self = Self(0x0031);
    pub const TalkDirect: Self = Self(0x0032);
    pub const SetAllegianceName: Self = Self(0x0033);
    pub const UseWithTarget: Self = Self(0x0035);
    pub const Use: Self = Self(0x0036);
    pub const SetAllegianceOfficer: Self = Self(0x003B);
    pub const SetAllegianceOfficerTitle: Self = Self(0x003C);
    pub const ListAllegianceOfficerTitles: Self = Self(0x003D);
    pub const ClearAllegianceOfficerTitles: Self = Self(0x003E);
    pub const DoAllegianceLockAction: Self = Self(0x003F);
    pub const SetAllegianceApprovedVassal: Self = Self(0x0040);
    pub const AllegianceChatGag: Self = Self(0x0041);
    pub const DoAllegianceHouseAction: Self = Self(0x0042);
    pub const RaiseVital: Self = Self(0x0044);
    pub const RaiseAttribute: Self = Self(0x0045);
    pub const RaiseSkill: Self = Self(0x0046);
    pub const TrainSkill: Self = Self(0x0047);
    pub const CastUntargetedSpell: Self = Self(0x0048);
    pub const CastTargetedSpell: Self = Self(0x004A);
    pub const ChangeCombatMode: Self = Self(0x0053);
    pub const StackableMerge: Self = Self(0x0054);
    pub const StackableSplitToContainer: Self = Self(0x0055);
    pub const StackableSplitTo3D: Self = Self(0x0056);
    pub const ModifyCharacterSquelch: Self = Self(0x0058);
    pub const ModifyAccountSquelch: Self = Self(0x0059);
    pub const ModifyGlobalSquelch: Self = Self(0x005B);
    pub const Tell: Self = Self(0x005D);
    pub const Buy: Self = Self(0x005F);
    pub const Sell: Self = Self(0x0060);
    pub const TeleToLifestone: Self = Self(0x0063);
    pub const LoginComplete: Self = Self(0x00A1);
    pub const FellowshipCreate: Self = Self(0x00A2);
    pub const FellowshipQuit: Self = Self(0x00A3);
    pub const FellowshipDismiss: Self = Self(0x00A4);
    pub const FellowshipRecruit: Self = Self(0x00A5);
    pub const FellowshipUpdateRequest: Self = Self(0x00A6);
    pub const BookData: Self = Self(0x00AA);
    pub const BookModifyPage: Self = Self(0x00AB);
    pub const BookAddPage: Self = Self(0x00AC);
    pub const BookDeletePage: Self = Self(0x00AD);
    pub const BookPageData: Self = Self(0x00AE);
    pub const SetInscription: Self = Self(0x00BF);
    pub const IdentifyObject: Self = Self(0x00C8);
    pub const GiveObjectRequest: Self = Self(0x00CD);
    pub const AdvocateTeleport: Self = Self(0x00D6);
    pub const AbuseLogRequest: Self = Self(0x0140);
    pub const AddChannel: Self = Self(0x0145);
    pub const RemoveChannel: Self = Self(0x0146);
    pub const ChatChannel: Self = Self(0x0147);
    pub const ListChannels: Self = Self(0x0148);
    pub const IndexChannels: Self = Self(0x0149);
    pub const NoLongerViewingContents: Self = Self(0x0195);
    pub const StackableSplitToWield: Self = Self(0x019B);
    pub const AddShortCut: Self = Self(0x019C);
    pub const RemoveShortCut: Self = Self(0x019D);
    pub const SetCharacterOptions: Self = Self(0x01A1);
    pub const RemoveSpellC2S: Self = Self(0x01A8);
    pub const CancelAttack: Self = Self(0x01B7);
    pub const QueryHealth: Self = Self(0x01BF);
    pub const QueryAge: Self = Self(0x01C2);
    pub const QueryBirth: Self = Self(0x01C4);
    pub const Emote: Self = Self(0x01DF);
    pub const SoulEmote: Self = Self(0x01E1);
    pub const AddSpellFavorite: Self = Self(0x01E3);
    pub const RemoveSpellFavorite: Self = Self(0x01E4);
    pub const PingRequest: Self = Self(0x01E9);
    pub const OpenTradeNegotiations: Self = Self(0x01F6);
    pub const CloseTradeNegotiations: Self = Self(0x01F7);
    pub const AddToTrade: Self = Self(0x01F8);
    pub const AcceptTrade: Self = Self(0x01FA);
    pub const DeclineTrade: Self = Self(0x01FB);
    pub const ResetTrade: Self = Self(0x0204);
    pub const ClearPlayerConsentList: Self = Self(0x0216);
    pub const DisplayPlayerConsentList: Self = Self(0x0217);
    pub const RemoveFromPlayerConsentList: Self = Self(0x0218);
    pub const AddPlayerPermission: Self = Self(0x0219);
    pub const RemovePlayerPermission: Self = Self(0x021A);
    pub const BuyHouse: Self = Self(0x021C);
    pub const HouseQuery: Self = Self(0x021E);
    pub const AbandonHouse: Self = Self(0x021F);
    pub const RentHouse: Self = Self(0x0221);
    pub const SetDesiredComponentLevel: Self = Self(0x0224);
    pub const AddPermanentGuest: Self = Self(0x0245);
    pub const RemovePermanentGuest: Self = Self(0x0246);
    pub const SetOpenHouseStatus: Self = Self(0x0247);
    pub const ChangeStoragePermission: Self = Self(0x0249);
    pub const BootSpecificHouseGuest: Self = Self(0x024A);
    pub const RemoveAllStoragePermission: Self = Self(0x024C);
    pub const RequestFullGuestList: Self = Self(0x024D);
    pub const SetMotd: Self = Self(0x0254);
    pub const QueryMotd: Self = Self(0x0255);
    pub const ClearMotd: Self = Self(0x0256);
    pub const QueryLord: Self = Self(0x0258);
    pub const AddAllStoragePermission: Self = Self(0x025C);
    pub const RemoveAllPermanentGuests: Self = Self(0x025E);
    pub const BootEveryone: Self = Self(0x025F);
    pub const TeleToHouse: Self = Self(0x0262);
    pub const QueryItemMana: Self = Self(0x0263);
    pub const SetHooksVisibility: Self = Self(0x0266);
    pub const ModifyAllegianceGuestPermission: Self = Self(0x0267);
    pub const ModifyAllegianceStoragePermission: Self = Self(0x0268);
    pub const ChessJoin: Self = Self(0x0269);
    pub const ChessQuit: Self = Self(0x026A);
    pub const ChessMove: Self = Self(0x026B);
    pub const ChessMovePass: Self = Self(0x026D);
    pub const ChessStalemate: Self = Self(0x026E);
    pub const ListAvailableHouses: Self = Self(0x0270);
    pub const ConfirmationResponse: Self = Self(0x0275);
    pub const BreakAllegianceBoot: Self = Self(0x0277);
    pub const TeleToMansion: Self = Self(0x0278);
    pub const Suicide: Self = Self(0x0279);
    pub const AllegianceInfoRequest: Self = Self(0x027B);
    pub const CreateTinkeringTool: Self = Self(0x027D);
    pub const SpellbookFilter: Self = Self(0x0286);
    pub const TeleToMarketPlace: Self = Self(0x028D);
    pub const EnterPkLite: Self = Self(0x028F);
    pub const FellowshipAssignNewLeader: Self = Self(0x0290);
    pub const FellowshipChangeOpenness: Self = Self(0x0291);
    pub const AllegianceChatBoot: Self = Self(0x02A0);
    pub const AddAllegianceBan: Self = Self(0x02A1);
    pub const RemoveAllegianceBan: Self = Self(0x02A2);
    pub const ListAllegianceBans: Self = Self(0x02A3);
    pub const RemoveAllegianceOfficer: Self = Self(0x02A5);
    pub const ListAllegianceOfficers: Self = Self(0x02A6);
    pub const ClearAllegianceOfficers: Self = Self(0x02A7);
    pub const RecallAllegianceHometown: Self = Self(0x02AB);
    pub const QueryPluginListResponse: Self = Self(0x02AF);
    pub const QueryPluginResponse: Self = Self(0x02B2);
    pub const FinishBarber: Self = Self(0x0311);
    pub const AbandonContract: Self = Self(0x0316);
    pub const Jump: Self = Self(0xF61B);
    pub const MoveToState: Self = Self(0xF61C);
    pub const DoMovementCommand: Self = Self(0xF61E);
    pub const TurnTo: Self = Self(0xF649);
    pub const StopMovementCommand: Self = Self(0xF661);
    pub const ForceObjectDescSend: Self = Self(0xF6EA);
    pub const ObjectCreate: Self = Self(0xF745);
    pub const ObjectDelete: Self = Self(0xF747);
    pub const MovementEvent: Self = Self(0xF74C);
    pub const ApplySoundEffect: Self = Self(0xF750);
    pub const AutonomyLevel: Self = Self(0xF752);
    pub const AutonomousPosition: Self = Self(0xF753);
    pub const ApplyVisualEffect: Self = Self(0xF755);
    pub const JumpNonAutonomous: Self = Self(0xF7C9);
    /// All upstream names, including aliases sharing a numeric identifier.
    pub const NAMED: &'static [(&'static str, Self)] = &[
        ("SetSingleCharacterOption", Self::SetSingleCharacterOption),
        ("TargetedMeleeAttack", Self::TargetedMeleeAttack),
        ("TargetedMissileAttack", Self::TargetedMissileAttack),
        ("SetAfkMode", Self::SetAfkMode),
        ("SetAfkMessage", Self::SetAfkMessage),
        ("Talk", Self::Talk),
        ("RemoveFriend", Self::RemoveFriend),
        ("AddFriend", Self::AddFriend),
        ("PutItemInContainer", Self::PutItemInContainer),
        ("GetAndWieldItem", Self::GetAndWieldItem),
        ("DropItem", Self::DropItem),
        ("SwearAllegiance", Self::SwearAllegiance),
        ("BreakAllegiance", Self::BreakAllegiance),
        ("AllegianceUpdateRequest", Self::AllegianceUpdateRequest),
        ("RemoveAllFriends", Self::RemoveAllFriends),
        ("TeleToPklArena", Self::TeleToPklArena),
        ("TeleToPkArena", Self::TeleToPkArena),
        ("TitleSet", Self::TitleSet),
        ("QueryAllegianceName", Self::QueryAllegianceName),
        ("ClearAllegianceName", Self::ClearAllegianceName),
        ("TalkDirect", Self::TalkDirect),
        ("SetAllegianceName", Self::SetAllegianceName),
        ("UseWithTarget", Self::UseWithTarget),
        ("Use", Self::Use),
        ("SetAllegianceOfficer", Self::SetAllegianceOfficer),
        ("SetAllegianceOfficerTitle", Self::SetAllegianceOfficerTitle),
        (
            "ListAllegianceOfficerTitles",
            Self::ListAllegianceOfficerTitles,
        ),
        (
            "ClearAllegianceOfficerTitles",
            Self::ClearAllegianceOfficerTitles,
        ),
        ("DoAllegianceLockAction", Self::DoAllegianceLockAction),
        (
            "SetAllegianceApprovedVassal",
            Self::SetAllegianceApprovedVassal,
        ),
        ("AllegianceChatGag", Self::AllegianceChatGag),
        ("DoAllegianceHouseAction", Self::DoAllegianceHouseAction),
        ("RaiseVital", Self::RaiseVital),
        ("RaiseAttribute", Self::RaiseAttribute),
        ("RaiseSkill", Self::RaiseSkill),
        ("TrainSkill", Self::TrainSkill),
        ("CastUntargetedSpell", Self::CastUntargetedSpell),
        ("CastTargetedSpell", Self::CastTargetedSpell),
        ("ChangeCombatMode", Self::ChangeCombatMode),
        ("StackableMerge", Self::StackableMerge),
        ("StackableSplitToContainer", Self::StackableSplitToContainer),
        ("StackableSplitTo3D", Self::StackableSplitTo3D),
        ("ModifyCharacterSquelch", Self::ModifyCharacterSquelch),
        ("ModifyAccountSquelch", Self::ModifyAccountSquelch),
        ("ModifyGlobalSquelch", Self::ModifyGlobalSquelch),
        ("Tell", Self::Tell),
        ("Buy", Self::Buy),
        ("Sell", Self::Sell),
        ("TeleToLifestone", Self::TeleToLifestone),
        ("LoginComplete", Self::LoginComplete),
        ("FellowshipCreate", Self::FellowshipCreate),
        ("FellowshipQuit", Self::FellowshipQuit),
        ("FellowshipDismiss", Self::FellowshipDismiss),
        ("FellowshipRecruit", Self::FellowshipRecruit),
        ("FellowshipUpdateRequest", Self::FellowshipUpdateRequest),
        ("BookData", Self::BookData),
        ("BookModifyPage", Self::BookModifyPage),
        ("BookAddPage", Self::BookAddPage),
        ("BookDeletePage", Self::BookDeletePage),
        ("BookPageData", Self::BookPageData),
        ("SetInscription", Self::SetInscription),
        ("IdentifyObject", Self::IdentifyObject),
        ("GiveObjectRequest", Self::GiveObjectRequest),
        ("AdvocateTeleport", Self::AdvocateTeleport),
        ("AbuseLogRequest", Self::AbuseLogRequest),
        ("AddChannel", Self::AddChannel),
        ("RemoveChannel", Self::RemoveChannel),
        ("ChatChannel", Self::ChatChannel),
        ("ListChannels", Self::ListChannels),
        ("IndexChannels", Self::IndexChannels),
        ("NoLongerViewingContents", Self::NoLongerViewingContents),
        ("StackableSplitToWield", Self::StackableSplitToWield),
        ("AddShortCut", Self::AddShortCut),
        ("RemoveShortCut", Self::RemoveShortCut),
        ("SetCharacterOptions", Self::SetCharacterOptions),
        ("RemoveSpellC2S", Self::RemoveSpellC2S),
        ("CancelAttack", Self::CancelAttack),
        ("QueryHealth", Self::QueryHealth),
        ("QueryAge", Self::QueryAge),
        ("QueryBirth", Self::QueryBirth),
        ("Emote", Self::Emote),
        ("SoulEmote", Self::SoulEmote),
        ("AddSpellFavorite", Self::AddSpellFavorite),
        ("RemoveSpellFavorite", Self::RemoveSpellFavorite),
        ("PingRequest", Self::PingRequest),
        ("OpenTradeNegotiations", Self::OpenTradeNegotiations),
        ("CloseTradeNegotiations", Self::CloseTradeNegotiations),
        ("AddToTrade", Self::AddToTrade),
        ("AcceptTrade", Self::AcceptTrade),
        ("DeclineTrade", Self::DeclineTrade),
        ("ResetTrade", Self::ResetTrade),
        ("ClearPlayerConsentList", Self::ClearPlayerConsentList),
        ("DisplayPlayerConsentList", Self::DisplayPlayerConsentList),
        (
            "RemoveFromPlayerConsentList",
            Self::RemoveFromPlayerConsentList,
        ),
        ("AddPlayerPermission", Self::AddPlayerPermission),
        ("RemovePlayerPermission", Self::RemovePlayerPermission),
        ("BuyHouse", Self::BuyHouse),
        ("HouseQuery", Self::HouseQuery),
        ("AbandonHouse", Self::AbandonHouse),
        ("RentHouse", Self::RentHouse),
        ("SetDesiredComponentLevel", Self::SetDesiredComponentLevel),
        ("AddPermanentGuest", Self::AddPermanentGuest),
        ("RemovePermanentGuest", Self::RemovePermanentGuest),
        ("SetOpenHouseStatus", Self::SetOpenHouseStatus),
        ("ChangeStoragePermission", Self::ChangeStoragePermission),
        ("BootSpecificHouseGuest", Self::BootSpecificHouseGuest),
        (
            "RemoveAllStoragePermission",
            Self::RemoveAllStoragePermission,
        ),
        ("RequestFullGuestList", Self::RequestFullGuestList),
        ("SetMotd", Self::SetMotd),
        ("QueryMotd", Self::QueryMotd),
        ("ClearMotd", Self::ClearMotd),
        ("QueryLord", Self::QueryLord),
        ("AddAllStoragePermission", Self::AddAllStoragePermission),
        ("RemoveAllPermanentGuests", Self::RemoveAllPermanentGuests),
        ("BootEveryone", Self::BootEveryone),
        ("TeleToHouse", Self::TeleToHouse),
        ("QueryItemMana", Self::QueryItemMana),
        ("SetHooksVisibility", Self::SetHooksVisibility),
        (
            "ModifyAllegianceGuestPermission",
            Self::ModifyAllegianceGuestPermission,
        ),
        (
            "ModifyAllegianceStoragePermission",
            Self::ModifyAllegianceStoragePermission,
        ),
        ("ChessJoin", Self::ChessJoin),
        ("ChessQuit", Self::ChessQuit),
        ("ChessMove", Self::ChessMove),
        ("ChessMovePass", Self::ChessMovePass),
        ("ChessStalemate", Self::ChessStalemate),
        ("ListAvailableHouses", Self::ListAvailableHouses),
        ("ConfirmationResponse", Self::ConfirmationResponse),
        ("BreakAllegianceBoot", Self::BreakAllegianceBoot),
        ("TeleToMansion", Self::TeleToMansion),
        ("Suicide", Self::Suicide),
        ("AllegianceInfoRequest", Self::AllegianceInfoRequest),
        ("CreateTinkeringTool", Self::CreateTinkeringTool),
        ("SpellbookFilter", Self::SpellbookFilter),
        ("TeleToMarketPlace", Self::TeleToMarketPlace),
        ("EnterPkLite", Self::EnterPkLite),
        ("FellowshipAssignNewLeader", Self::FellowshipAssignNewLeader),
        ("FellowshipChangeOpenness", Self::FellowshipChangeOpenness),
        ("AllegianceChatBoot", Self::AllegianceChatBoot),
        ("AddAllegianceBan", Self::AddAllegianceBan),
        ("RemoveAllegianceBan", Self::RemoveAllegianceBan),
        ("ListAllegianceBans", Self::ListAllegianceBans),
        ("RemoveAllegianceOfficer", Self::RemoveAllegianceOfficer),
        ("ListAllegianceOfficers", Self::ListAllegianceOfficers),
        ("ClearAllegianceOfficers", Self::ClearAllegianceOfficers),
        ("RecallAllegianceHometown", Self::RecallAllegianceHometown),
        ("QueryPluginListResponse", Self::QueryPluginListResponse),
        ("QueryPluginResponse", Self::QueryPluginResponse),
        ("FinishBarber", Self::FinishBarber),
        ("AbandonContract", Self::AbandonContract),
        ("Jump", Self::Jump),
        ("MoveToState", Self::MoveToState),
        ("DoMovementCommand", Self::DoMovementCommand),
        ("TurnTo", Self::TurnTo),
        ("StopMovementCommand", Self::StopMovementCommand),
        ("ForceObjectDescSend", Self::ForceObjectDescSend),
        ("ObjectCreate", Self::ObjectCreate),
        ("ObjectDelete", Self::ObjectDelete),
        ("MovementEvent", Self::MovementEvent),
        ("ApplySoundEffect", Self::ApplySoundEffect),
        ("AutonomyLevel", Self::AutonomyLevel),
        ("AutonomousPosition", Self::AutonomousPosition),
        ("ApplyVisualEffect", Self::ApplyVisualEffect),
        ("JumpNonAutonomous", Self::JumpNonAutonomous),
    ];
}
