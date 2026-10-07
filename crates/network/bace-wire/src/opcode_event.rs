//! Official ACE `GameEvent/GameEventType.cs` at 47edade3bd3f6044b676d4eb877c4965c7eda62b.
//! Identifiers alone do not establish payload implementation.

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct GameEventType(pub u32);

#[allow(non_upper_case_globals)]
impl GameEventType {
    pub const AllegianceUpdateAborted: Self = Self(0x0003);
    pub const PopupString: Self = Self(0x0004);
    pub const PlayerDescription: Self = Self(0x0013);
    pub const AllegianceUpdate: Self = Self(0x0020);
    pub const FriendsListUpdate: Self = Self(0x0021);
    pub const InventoryPutObjInContainer: Self = Self(0x0022);
    pub const WieldObject: Self = Self(0x0023);
    pub const CharacterTitle: Self = Self(0x0029);
    pub const UpdateTitle: Self = Self(0x002B);
    pub const CloseGroundContainer: Self = Self(0x0052);
    pub const VendorInfoEvent: Self = Self(0x0062);
    pub const ApproachVendor: Self = Self(0x0062);
    pub const StartBarber: Self = Self(0x0075);
    pub const InventoryServerSaveFailed: Self = Self(0x00A0);
    pub const FellowshipQuit: Self = Self(0x00A3);
    pub const FellowshipDismiss: Self = Self(0x00A4);
    pub const BookDataResponse: Self = Self(0x00B4);
    pub const BookModifyPageResponse: Self = Self(0x00B5);
    pub const BookAddPageResponse: Self = Self(0x00B6);
    pub const BookDeletePageResponse: Self = Self(0x00B7);
    pub const BookPageDataResponse: Self = Self(0x00B8);
    pub const GetInscriptionResponse: Self = Self(0x00C3);
    pub const IdentifyObjectResponse: Self = Self(0x00C9);
    pub const ChannelBroadcast: Self = Self(0x0147);
    pub const ChannelList: Self = Self(0x0148);
    pub const ChannelIndex: Self = Self(0x0149);
    pub const ViewContents: Self = Self(0x0196);
    pub const InventoryPutObjectIn3D: Self = Self(0x019A);
    pub const AttackDone: Self = Self(0x01A7);
    pub const MagicRemoveSpell: Self = Self(0x01A8);
    pub const VictimNotification: Self = Self(0x01AC);
    pub const KillerNotification: Self = Self(0x01AD);
    pub const AttackerNotification: Self = Self(0x01B1);
    pub const DefenderNotification: Self = Self(0x01B2);
    pub const EvasionAttackerNotification: Self = Self(0x01B3);
    pub const EvasionDefenderNotification: Self = Self(0x01B4);
    pub const CombatCommenceAttack: Self = Self(0x01B8);
    pub const UpdateHealth: Self = Self(0x01C0);
    pub const QueryAgeResponse: Self = Self(0x01C3);
    pub const UseDone: Self = Self(0x01C7);
    pub const AllegianceAllegianceUpdateDone: Self = Self(0x01C8);
    pub const FellowshipFellowUpdateDone: Self = Self(0x01C9);
    pub const FellowshipFellowStatsDone: Self = Self(0x01CA);
    pub const ItemAppraiseDone: Self = Self(0x01CB);
    pub const Emote: Self = Self(0x01E2);
    pub const PingResponse: Self = Self(0x01EA);
    pub const SetSquelchDB: Self = Self(0x01F4);
    pub const RegisterTrade: Self = Self(0x01FD);
    pub const OpenTrade: Self = Self(0x01FE);
    pub const CloseTrade: Self = Self(0x01FF);
    pub const AddToTrade: Self = Self(0x0200);
    pub const RemoveFromTrade: Self = Self(0x0201);
    pub const AcceptTrade: Self = Self(0x0202);
    pub const DeclineTrade: Self = Self(0x0203);
    pub const ResetTrade: Self = Self(0x0205);
    pub const TradeFailure: Self = Self(0x0207);
    pub const ClearTradeAcceptance: Self = Self(0x0208);
    pub const HouseProfile: Self = Self(0x021D);
    pub const HouseData: Self = Self(0x0225);
    pub const HouseStatus: Self = Self(0x0226);
    pub const UpdateRentTime: Self = Self(0x0227);
    pub const UpdateRentPayment: Self = Self(0x0228);
    pub const HouseUpdateRestrictions: Self = Self(0x0248);
    pub const UpdateHAR: Self = Self(0x0257);
    pub const HouseTransaction: Self = Self(0x0259);
    pub const QueryItemManaResponse: Self = Self(0x0264);
    pub const AvailableHouses: Self = Self(0x0271);
    pub const CharacterConfirmationRequest: Self = Self(0x0274);
    pub const CharacterConfirmationDone: Self = Self(0x0276);
    pub const AllegianceLoginNotification: Self = Self(0x027A);
    pub const AllegianceInfoResponse: Self = Self(0x027C);
    pub const JoinGameResponse: Self = Self(0x0281);
    pub const StartGame: Self = Self(0x0282);
    pub const MoveResponse: Self = Self(0x0283);
    pub const OpponentTurn: Self = Self(0x0284);
    pub const OpponentStalemate: Self = Self(0x0285);
    pub const WeenieError: Self = Self(0x028A);
    pub const WeenieErrorWithString: Self = Self(0x028B);
    pub const GameOver: Self = Self(0x028C);
    pub const SetTurbineChatChannels: Self = Self(0x0295);
    pub const AdminQueryPluginList: Self = Self(0x02AE);
    pub const AdminQueryPlugin: Self = Self(0x02B1);
    pub const AdminQueryPluginResponse: Self = Self(0x02B3);
    pub const SalvageOperationsResult: Self = Self(0x02B4);
    pub const Tell: Self = Self(0x02BD);
    pub const FellowshipFullUpdate: Self = Self(0x02BE);
    pub const FellowshipDisband: Self = Self(0x02BF);
    pub const FellowshipUpdateFellow: Self = Self(0x02C0);
    pub const MagicUpdateSpell: Self = Self(0x02C1);
    pub const MagicUpdateEnchantment: Self = Self(0x02C2);
    pub const MagicRemoveEnchantment: Self = Self(0x02C3);
    pub const MagicUpdateMultipleEnchantments: Self = Self(0x02C4);
    pub const MagicRemoveMultipleEnchantments: Self = Self(0x02C5);
    pub const MagicPurgeEnchantments: Self = Self(0x02C6);
    pub const MagicDispelEnchantment: Self = Self(0x02C7);
    pub const MagicDispelMultipleEnchantments: Self = Self(0x02C8);
    pub const MiscPortalStormBrewing: Self = Self(0x02C9);
    pub const MiscPortalStormImminent: Self = Self(0x02CA);
    pub const MiscPortalStorm: Self = Self(0x02CB);
    pub const MiscPortalstormSubsided: Self = Self(0x02CC);
    pub const CommunicationTransientString: Self = Self(0x02EB);
    pub const MagicPurgeBadEnchantments: Self = Self(0x0312);
    pub const SendClientContractTrackerTable: Self = Self(0x0314);
    pub const SendClientContractTracker: Self = Self(0x0315);
    /// All upstream names, including aliases sharing a numeric identifier.
    pub const NAMED: &'static [(&'static str, Self)] = &[
        ("AllegianceUpdateAborted", Self::AllegianceUpdateAborted),
        ("PopupString", Self::PopupString),
        ("PlayerDescription", Self::PlayerDescription),
        ("AllegianceUpdate", Self::AllegianceUpdate),
        ("FriendsListUpdate", Self::FriendsListUpdate),
        (
            "InventoryPutObjInContainer",
            Self::InventoryPutObjInContainer,
        ),
        ("WieldObject", Self::WieldObject),
        ("CharacterTitle", Self::CharacterTitle),
        ("UpdateTitle", Self::UpdateTitle),
        ("CloseGroundContainer", Self::CloseGroundContainer),
        ("VendorInfoEvent", Self::VendorInfoEvent),
        ("ApproachVendor", Self::ApproachVendor),
        ("StartBarber", Self::StartBarber),
        ("InventoryServerSaveFailed", Self::InventoryServerSaveFailed),
        ("FellowshipQuit", Self::FellowshipQuit),
        ("FellowshipDismiss", Self::FellowshipDismiss),
        ("BookDataResponse", Self::BookDataResponse),
        ("BookModifyPageResponse", Self::BookModifyPageResponse),
        ("BookAddPageResponse", Self::BookAddPageResponse),
        ("BookDeletePageResponse", Self::BookDeletePageResponse),
        ("BookPageDataResponse", Self::BookPageDataResponse),
        ("GetInscriptionResponse", Self::GetInscriptionResponse),
        ("IdentifyObjectResponse", Self::IdentifyObjectResponse),
        ("ChannelBroadcast", Self::ChannelBroadcast),
        ("ChannelList", Self::ChannelList),
        ("ChannelIndex", Self::ChannelIndex),
        ("ViewContents", Self::ViewContents),
        ("InventoryPutObjectIn3D", Self::InventoryPutObjectIn3D),
        ("AttackDone", Self::AttackDone),
        ("MagicRemoveSpell", Self::MagicRemoveSpell),
        ("VictimNotification", Self::VictimNotification),
        ("KillerNotification", Self::KillerNotification),
        ("AttackerNotification", Self::AttackerNotification),
        ("DefenderNotification", Self::DefenderNotification),
        (
            "EvasionAttackerNotification",
            Self::EvasionAttackerNotification,
        ),
        (
            "EvasionDefenderNotification",
            Self::EvasionDefenderNotification,
        ),
        ("CombatCommenceAttack", Self::CombatCommenceAttack),
        ("UpdateHealth", Self::UpdateHealth),
        ("QueryAgeResponse", Self::QueryAgeResponse),
        ("UseDone", Self::UseDone),
        (
            "AllegianceAllegianceUpdateDone",
            Self::AllegianceAllegianceUpdateDone,
        ),
        (
            "FellowshipFellowUpdateDone",
            Self::FellowshipFellowUpdateDone,
        ),
        ("FellowshipFellowStatsDone", Self::FellowshipFellowStatsDone),
        ("ItemAppraiseDone", Self::ItemAppraiseDone),
        ("Emote", Self::Emote),
        ("PingResponse", Self::PingResponse),
        ("SetSquelchDB", Self::SetSquelchDB),
        ("RegisterTrade", Self::RegisterTrade),
        ("OpenTrade", Self::OpenTrade),
        ("CloseTrade", Self::CloseTrade),
        ("AddToTrade", Self::AddToTrade),
        ("RemoveFromTrade", Self::RemoveFromTrade),
        ("AcceptTrade", Self::AcceptTrade),
        ("DeclineTrade", Self::DeclineTrade),
        ("ResetTrade", Self::ResetTrade),
        ("TradeFailure", Self::TradeFailure),
        ("ClearTradeAcceptance", Self::ClearTradeAcceptance),
        ("HouseProfile", Self::HouseProfile),
        ("HouseData", Self::HouseData),
        ("HouseStatus", Self::HouseStatus),
        ("UpdateRentTime", Self::UpdateRentTime),
        ("UpdateRentPayment", Self::UpdateRentPayment),
        ("HouseUpdateRestrictions", Self::HouseUpdateRestrictions),
        ("UpdateHAR", Self::UpdateHAR),
        ("HouseTransaction", Self::HouseTransaction),
        ("QueryItemManaResponse", Self::QueryItemManaResponse),
        ("AvailableHouses", Self::AvailableHouses),
        (
            "CharacterConfirmationRequest",
            Self::CharacterConfirmationRequest,
        ),
        ("CharacterConfirmationDone", Self::CharacterConfirmationDone),
        (
            "AllegianceLoginNotification",
            Self::AllegianceLoginNotification,
        ),
        ("AllegianceInfoResponse", Self::AllegianceInfoResponse),
        ("JoinGameResponse", Self::JoinGameResponse),
        ("StartGame", Self::StartGame),
        ("MoveResponse", Self::MoveResponse),
        ("OpponentTurn", Self::OpponentTurn),
        ("OpponentStalemate", Self::OpponentStalemate),
        ("WeenieError", Self::WeenieError),
        ("WeenieErrorWithString", Self::WeenieErrorWithString),
        ("GameOver", Self::GameOver),
        ("SetTurbineChatChannels", Self::SetTurbineChatChannels),
        ("AdminQueryPluginList", Self::AdminQueryPluginList),
        ("AdminQueryPlugin", Self::AdminQueryPlugin),
        ("AdminQueryPluginResponse", Self::AdminQueryPluginResponse),
        ("SalvageOperationsResult", Self::SalvageOperationsResult),
        ("Tell", Self::Tell),
        ("FellowshipFullUpdate", Self::FellowshipFullUpdate),
        ("FellowshipDisband", Self::FellowshipDisband),
        ("FellowshipUpdateFellow", Self::FellowshipUpdateFellow),
        ("MagicUpdateSpell", Self::MagicUpdateSpell),
        ("MagicUpdateEnchantment", Self::MagicUpdateEnchantment),
        ("MagicRemoveEnchantment", Self::MagicRemoveEnchantment),
        (
            "MagicUpdateMultipleEnchantments",
            Self::MagicUpdateMultipleEnchantments,
        ),
        (
            "MagicRemoveMultipleEnchantments",
            Self::MagicRemoveMultipleEnchantments,
        ),
        ("MagicPurgeEnchantments", Self::MagicPurgeEnchantments),
        ("MagicDispelEnchantment", Self::MagicDispelEnchantment),
        (
            "MagicDispelMultipleEnchantments",
            Self::MagicDispelMultipleEnchantments,
        ),
        ("MiscPortalStormBrewing", Self::MiscPortalStormBrewing),
        ("MiscPortalStormImminent", Self::MiscPortalStormImminent),
        ("MiscPortalStorm", Self::MiscPortalStorm),
        ("MiscPortalstormSubsided", Self::MiscPortalstormSubsided),
        (
            "CommunicationTransientString",
            Self::CommunicationTransientString,
        ),
        ("MagicPurgeBadEnchantments", Self::MagicPurgeBadEnchantments),
        (
            "SendClientContractTrackerTable",
            Self::SendClientContractTrackerTable,
        ),
        ("SendClientContractTracker", Self::SendClientContractTracker),
    ];
}
