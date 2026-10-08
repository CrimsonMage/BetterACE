#!/usr/bin/env python3
"""Compile unmodified ACE message serializers; synthetic harness supplies domain inputs.
No DAT files, player captures or real accounts are used.
"""
import argparse
import concurrent.futures
import hashlib
import json
from pathlib import Path
import subprocess
import tempfile
import urllib.request
import urllib.error
import message_social_extract

PIN = '47edade3bd3f6044b676d4eb877c4965c7eda62b'
NETWORK = 'Source/ACE.Server/Network/'
MESSAGES = ['CharacterError', 'CharacterList', 'CharacterCreateResponse', 'CharacterRestore',
            'CharacterDelete', 'CharacterLogOff', 'CharacterEnterWorldServerReady',
            'ServerName', 'SystemChat', 'HearSpeech', 'HearRangedSpeech', 'EmoteText',
            'SoulEmote', 'BootAccount', 'AccountBanned', 'DDDBeginDDD', 'DDDEndDDD', 'DDDErrorMessage',
            'DDDInterrogation', 'DDDDataMessage']
MESSAGES += [scope + name for scope in ['Private', 'Public'] for name in ['UpdatePropertyInt', 'UpdatePropertyInt64', 'UpdatePropertyBool', 'UpdatePropertyFloat', 'UpdatePropertyString', 'UpdateDataID', 'UpdateInstanceID']]
MESSAGES += ['PrivateUpdateAttribute', 'PrivateUpdateVital', 'PublicUpdateVital', 'PrivateUpdateSkill', 'PrivateUpdateAttribute2ndLevel']
EVENTS = ['FellowshipFellowUpdateDone', 'WeenieError', 'UseDone', 'PingResponse']
SOURCES = [NETWORK + path for path in [
    'GameMessages/GameMessage.cs', 'GameMessages/GameMessageOpcode.cs',
    'GameAction/GameActionType.cs', 'GameAction/GameActionAttribute.cs',
    'GameAction/Actions/GameActionRaiseAttribute.cs', 'GameAction/Actions/GameActionRaiseVital.cs',
    'GameAction/Actions/GameActionRaiseSkill.cs', 'GameAction/Actions/GameActionTrainSkill.cs', 'GameAction/GameActionPacket.cs', 'GameMessages/GameMessageAttribute.cs', 'ClientMessage.cs', 'GameEvent/GameEventType.cs',
    'GameEvent/GameEventMessage.cs', 'GameMessageGroup.cs', 'Extensions.cs',
    'Enum/CharacterError.cs', 'Enum/CharacterGenerationVerificationResponse.cs',
    'Enum/SessionState.cs', 'Sequence/SequenceType.cs',
    'Structure/CAllIterationList.cs', 'Structure/PTaggedIterationList.cs', 'Structure/CMostlyConsecutiveIntSet.cs',
]] + [NETWORK + 'GameMessages/Messages/GameMessage' + name + '.cs' for name in MESSAGES]
SOURCES += [NETWORK + 'GameEvent/Events/GameEvent' + name + '.cs' for name in EVENTS]
SOURCES += ['Source/ACE.Entity/CharacterCreateInfo.cs', 'Source/ACE.Entity/Appearance.cs', 'Source/ACE.Entity/Enum/HeritageGroup.cs', 'Source/ACE.Common/Extensions/BinaryReaderExtensions.cs', 'Source/ACE.Entity/ObjectGuid.cs', 'Source/ACE.Entity/Enum/ChatMessageType.cs',
            'Source/ACE.Entity/Enum/AccessLevel.cs', 'Source/ACE.Entity/Enum/SquelchMask.cs', 'Source/ACE.Entity/Enum/WeenieError.cs',
            'Source/ACE.DatLoader/DatDatabaseType.cs']

SOURCES += [NETWORK + 'Structure/' + name + '.cs' for name in ['PositionPack', 'JumpPack', 'Origin', 'Extensions']]
SOURCES += [NETWORK + 'Motion/' + name + '.cs' for name in ['RawMotionState', 'MoveToState', 'MotionItem', 'MovementData', 'InterpretedMotionState', 'MovementInvalid', 'MoveToObject', 'MoveToPosition', 'MoveToParameters', 'TurnToObject', 'TurnToHeading', 'TurnToParameters']]
SOURCES += [NETWORK + 'GameMessages/Messages/GameMessage' + name + '.cs' for name in ['UpdatePosition', 'VectorUpdate', 'AutonomousPosition', 'UpdateMotion']]
SOURCES += [NETWORK + 'GameAction/Actions/GameAction' + name + '.cs' for name in ['Jump', 'AutonomousPosition']]
SOURCES += [NETWORK + 'Enum/RawMotionFlags.cs', 'Source/ACE.Entity/Position.cs', 'Source/ACE.Entity/LandblockId.cs', 'Source/ACE.Server/Entity/SoulEmote.cs']
SOURCES += ['Source/ACE.Entity/Enum/' + name + '.cs' for name in ['PositionFlags', 'MotionCommand', 'MotionStance', 'MovementStateFlag', 'MovementType', 'MotionFlags', 'MovementParams', 'HoldKey', 'Placement']]
PRIMITIVE_SOURCE = NETWORK + 'Structure/AllegianceHierarchy.cs'


SOURCES += [NETWORK + 'GameMessages/Messages/GameMessage' + name + '.cs' for name in ['CreateObject','UpdateObject','DeleteObject','ObjDescEvent','PlayerCreate','SetState','ParentEvent','PickupEvent','InventoryRemoveObject','SetStackSize','PlayerTeleport']]
SOURCES += ['Source/ACE.Entity/ObjDesc.cs','Source/ACE.Entity/Models/PropertiesPalette.cs','Source/ACE.Entity/Models/PropertiesTextureMap.cs','Source/ACE.Entity/Models/PropertiesAnimPart.cs','Source/ACE.Server/Entity/HeldItem.cs']
SOURCES += ['Source/ACE.Entity/Enum/' + name + '.cs' for name in ['WeenieHeaderFlags','ObjectDescriptionFlag','PhysicsDescriptionFlag','PhysicsState','HouseType','ParentLocation','CloakStatus','EquipMask']]
SOURCES += [NETWORK + 'Structure/' + name + '.cs' for name in ['RestrictionDB','HashComparer','PackableHashTable']]
OBJECT_SOURCE = 'Source/ACE.Server/WorldObjects/WorldObject_Networking.cs'
OBJECT_METHODS = ['public virtual void SerializeUpdateObject(', 'public virtual void SerializeCreateObject(', 'public virtual void SerializeGameDataOnly(', 'public virtual void SerializeUpdateModelData(', 'private void SerializeCreateObject(', 'private void SerializeModelData(', 'private void SerializePhysicsData(']

def extract_method(source, signature):
    start = source.index(signature)
    brace = source.index('{', start)
    level = 1
    end = brace + 1
    while level:
        if source[end] == '{': level += 1
        elif source[end] == '}': level -= 1
        end += 1
    return source[start:end]


SOCIAL_ACTIONS = ['AddFriend','RemoveFriend','RemoveAllFriends','ModifyGlobalSquelch','ModifyCharacterSquelch','ModifyAccountSquelch','Emote','SoulEmote','SetAFKMode','SetAFKMessage']
SOURCES += [NETWORK + 'GameAction/Actions/GameAction' + name + '.cs' for name in SOCIAL_ACTIONS]
SOURCES += [NETWORK + 'GameMessages/Messages/GameMessageTurbineChat.cs']
SOURCES += [NETWORK + 'GameEvent/Events/GameEvent' + name + '.cs' for name in ['Tell','SetTurbineChatChannels','FriendsListUpdate','CommunicationSetSquelch','ChannelBroadcast','CommunicationTransientString','ChannelList','ChannelIndex']]
SOURCES += [NETWORK + 'Structure/SquelchInfo.cs','Source/ACE.Server/Entity/TurbineChatChannel.cs','Source/ACE.Database/Models/Shard/CharacterPropertiesFriendList.cs']
SOURCES += ['Source/ACE.Entity/Enum/' + name + '.cs' for name in ['ChatNetworkBlobType','ChatNetworkBlobDispatchType','ChatType','Channel','CreatureType']]

INVENTORY_ACTIONS = ['PutItemInContainer','GetAndWieldItem','DropItem','UseItem','UseWithTarget','StackableMerge','StackableSplitToContainer','StackableSplitTo3D','StackableSplitToWield','GiveObjectRequest','BuyItems','SellItems','OpenTradeNegotiations','CloseTradeNegotiations','AddToTrade','AcceptTrade','DeclineTrade','ResetTrade','NoLongerViewingContents']
SOURCES += [NETWORK + 'GameAction/Actions/GameAction' + name + '.cs' for name in INVENTORY_ACTIONS]
SOURCES += [NETWORK + 'GameEvent/Events/GameEvent' + name + '.cs' for name in ['ItemServerSaysMoveItem','ItemServerSaysContainId','WieldItem','ViewContents','CloseGroundContainer','InventoryServerSaveFailed','RegisterTrade','AddToTrade','AcceptTrade','DeclineTrade','ResetTrade','CloseTrade','ClearTradeAcceptance','TradeFailure','ApproachVendor']]
SOURCES += ['Source/ACE.Server/Entity/ItemProfile.cs']
SOURCES += ['Source/ACE.Entity/Enum/' + name + '.cs' for name in ['ContainerType','TradeSide','EndTradeReason','WeenieType']]

SOURCES += [NETWORK + 'GameAction/Actions/GameAction' + name + '.cs' for name in ['TargetedMeleeAttack','ChangeCombatMode','CancelAttack','QueryHealth']]
SOURCES += [NETWORK + 'GameEvent/Events/GameEvent' + name + '.cs' for name in ['AttackDone','AttackerNotification','DefenderNotification','EvasionAttackerNotification','EvasionDefenderNotification','CombatCommenceAttack','UpdateHealth','KillerNotification','VictimNotification']]
SOURCES += [NETWORK + 'GameMessages/Messages/GameMessage' + name + '.cs' for name in ['Sound','Script','PlayerKilled']]
SOURCES += ['Source/ACE.Entity/Enum/' + name + '.cs' for name in ['CombatMode','DamageType','AttackConditions','Sound','PlayScript']]
SOURCES += [NETWORK + 'GameAction/Actions/GameActionLoginComplete.cs', NETWORK + 'Handlers/ControlHandler.cs', NETWORK + 'Enum/DamageLocation.cs', 'Source/ACE.Common/ThreadSafeRandom.cs', 'Source/ACE.Common/Extensions/EnumHelper.cs']

SOURCES += [NETWORK + 'GameEvent/Events/GameEventCharacterTitle.cs', NETWORK + 'GameEvent/Events/GameEventPlayerDescription.cs', NETWORK + 'Structure/Shortcut.cs', NETWORK + 'Structure/LayeredSpell.cs']
SOURCES += ['Source/ACE.Entity/Enum/AttributeCache.cs', 'Source/ACE.Entity/Enum/CharacterOptionDataFlag.cs', 'Source/ACE.Database/Models/Shard/CharacterPropertiesSpellBar.cs', 'Source/ACE.Database/Models/Shard/CharacterPropertiesTitleBook.cs']

# These sources establish handler prefixes and domain input widths supplied by
# harness adapters. They are hashed and verified, not compiled into the harness.
EVIDENCE_SOURCES = ['Source/ACE.Server/WorldObjects/Player_Networking.cs', 'Source/ACE.Entity/Enum/Properties/SendOnLoginProperties.cs', 'Source/ACE.Entity/Enum/Properties/PositionType.cs', 'Source/ACE.Server/WorldObjects/Vendor.cs', OBJECT_SOURCE, 'Source/ACE.Server/WorldObjects/WorldObject_Properties.cs', PRIMITIVE_SOURCE, 'Source/ACE.Server/Physics/PhysicsEngine.cs', NETWORK + 'Handlers/CharacterHandler.cs', NETWORK + 'Handlers/DDDHandler.cs',
    'Source/ACE.Server/WorldObjects/Entity/CreatureAttribute.cs',
    'Source/ACE.Server/WorldObjects/Entity/CreatureVital.cs',
    'Source/ACE.Server/WorldObjects/Entity/CreatureSkill.cs',
    'Source/ACE.Entity/Models/PropertiesSkill.cs', NETWORK + 'Sequence/SequenceManager.cs'] + message_social_extract.SOURCES

parser = argparse.ArgumentParser()
parser.add_argument('--source', type=Path)
parser.add_argument('--dotnet', default='dotnet')
args = parser.parse_args()
root = Path(__file__).resolve().parent

def fetch(path):
    for attempt in range(3):
        try:
            official = urllib.request.urlopen(f'https://raw.githubusercontent.com/ACEmulator/ACE/{PIN}/{path}', timeout=15).read()
            break
        except (urllib.error.URLError, TimeoutError):
            if attempt == 2:
                raise
    data = (args.source / path).read_bytes() if args.source else official
    if data != official:
        raise SystemExit(f'Not pinned official source: {path}')
    return path, data

with tempfile.TemporaryDirectory(prefix='bace-message-oracle-') as tmp:
    build = Path(tmp)
    hashes = {}
    source_bytes = {}
    with concurrent.futures.ThreadPoolExecutor(max_workers=8) as pool:
        for path, data in pool.map(fetch, SOURCES + EVIDENCE_SOURCES):
            hashes[path] = hashlib.sha256(data).hexdigest()
            source_bytes[path] = data
            if path == 'Source/ACE.Server/WorldObjects/Vendor.cs':
                method = extract_method(data.decode('utf-8-sig'), 'public void forEachItem(Action<WorldObject> action)')
                (build / 'ExtractedVendorTraversal.cs').write_text('using System; namespace ACE.Server.WorldObjects { public partial class Vendor { ' + method + ' } }')
            if path == OBJECT_SOURCE:
                source = data.decode('utf-8-sig')
                methods = '\n'.join(extract_method(source, signature) for signature in OBJECT_METHODS)
                (build / 'ExtractedObjectSerializers.cs').write_text('using System; using System.IO; using ACE.Entity; using ACE.Entity.Enum; using ACE.Server.Entity; using ACE.Server.Network; using ACE.Server.Network.Sequence; using ACE.Server.Network.Structure; using ACE.Server.Managers; namespace ACE.Server.WorldObjects { public partial class WorldObject { ' + methods + ' } }')
            if path == PRIMITIVE_SOURCE:
                source = data.decode('utf-8-sig')
                signature = 'public static void Write(this BinaryWriter writer, Vector3 v)'
                start = source.index(signature)
                end = source.index('}', start) + 1
                method = source[start:end]
                (build / 'ExtractedVectorWriter.cs').write_text('using System.IO; using System.Numerics; namespace ACE.Server.Network.Structure { public static class ExtractedVectorWriter { ' + method + ' } }')
            if path in SOURCES:
                (build / path.replace('/', '__')).write_bytes(data)
    player_networking = source_bytes['Source/ACE.Server/WorldObjects/Player_Networking.cs'].decode('utf-8-sig')
    login_methods = '\n'.join(extract_method(player_networking, signature) for signature in ['private void SendSelf()', 'public void SendInventoryAndWieldedItems()'])
    (build / 'ExtractedLoginProjection.cs').write_text('using ACE.Server.Network.GameEvent.Events; using ACE.Server.Network.GameMessages.Messages; namespace ACE.Server.WorldObjects {public partial class Player {' + login_methods + '}}')
    lifecycle_source = source_bytes[NETWORK + 'Handlers/CharacterHandler.cs'].decode('utf-8-sig')
    readers = []
    for method, result in [('CharacterEnterWorld', 'new { character_id = guid, account = clientString }'), ('CharacterDelete', 'new { account = clientString, slot = characterSlot }'), ('CharacterRestore', 'new { character_id = guid }')]:
        body = extract_method(lifecycle_source, 'public static void ' + method + '(')
        prefix = body[body.index('{')+1:body.index('if (ServerManager.ShutdownInProgress)')]
        readers.append('public static object ' + method + '(ClientMessage message) {' + prefix + ' return ' + result + '; }')
    (build / 'ExtractedLifecycleReaders.cs').write_text('using ACE.Server.Network; using ACE.Common.Extensions; public static class ExtractedLifecycleReaders {' + '\n'.join(readers) + '}')
    social_readers, squelch_writer = message_social_extract.generate(source_bytes, extract_method)
    (build / 'ExtractedSocialReaders.cs').write_text(social_readers)
    accept_source = source_bytes[NETWORK + 'GameAction/Actions/GameActionAcceptTrade.cs'].decode('utf-8-sig')
    accept_prefix = accept_source[accept_source.index('uint partnerGuid ='):accept_source.index('session.Player.HandleActionAcceptTrade();')]
    (build / 'ExtractedAcceptTrade.cs').write_text('using System; using ACE.Server.Network; public static class ExtractedAcceptTrade { public static object Read(ClientMessage message) { ' + accept_prefix + ' return new { partner_id = partnerGuid, trade_stamp = tradeStamp, status = tradeStatus, initiator_id = initiatorGuid, initiator_accepts = initatorAccepts, partner_accepts = partnerAccepts }; } }')
    (build / 'ExtractedSquelchWriter.cs').write_text(squelch_writer)
    (build / 'Oracle.csproj').write_text('<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><OutputType>Exe</OutputType><TargetFramework>net10.0</TargetFramework><CheckForOverflowUnderflow>false</CheckForOverflowUnderflow></PropertyGroup></Project>')
    (build / 'Program.cs').write_bytes((root / 'message_Program.cs').read_bytes())
    (build / 'MovementHarness.cs').write_bytes((root / 'message_Movement.cs').read_bytes())
    (build / 'ObjectHarness.cs').write_bytes((root / 'message_Object.cs').read_bytes())
    (build / 'SocialHarness.cs').write_bytes((root / 'message_Social.cs').read_bytes())
    (build / 'InventoryHarness.cs').write_bytes((root / 'message_Inventory.cs').read_bytes())
    (build / 'CharacterLifecycleHarness.cs').write_bytes((root / 'message_CharacterLifecycle.cs').read_bytes())
    (build / 'PlayerDescriptionHarness.cs').write_bytes((root / 'message_PlayerDescription.cs').read_bytes())
    (build / 'CombatHarness.cs').write_bytes((root / 'message_Combat.cs').read_bytes())
    subprocess.run([args.dotnet, 'build', '--nologo', '-o', str(build / 'out'), str(build / 'Oracle.csproj')], check=True)
    result = subprocess.check_output([args.dotnet, str(build / 'out/Oracle.dll')], text=True)
    fixtures = {'repository': 'https://github.com/ACEmulator/ACE', 'commit': PIN,
                'source_sha256': hashes, 'compiled_sources': SOURCES, 'extracted_methods': {PRIMITIVE_SOURCE: ['Write(BinaryWriter, Vector3)'], OBJECT_SOURCE: OBJECT_METHODS, 'Source/ACE.Server/WorldObjects/Vendor.cs': ['forEachItem(Action<WorldObject>)']}, 'evidence_only_sources': EVIDENCE_SOURCES, 'social_parsing_prefixes': list(message_social_extract.PREFIXES) + ['Turbine header and request payload'], 'social_extracted_classes': ['SquelchDBExtensions'], 'inventory_parsing_prefixes': ['GameActionAcceptTrade.Handle before HandleActionAcceptTrade'], 'login_projection_methods': ['Player_Networking.SendSelf', 'Player_Networking.SendInventoryAndWieldedItems'], 'character_lifecycle_parsing_prefixes': ['CharacterEnterWorld', 'CharacterDelete', 'CharacterRestore'], 'player_description_scope': 'Full original serializer; synthetic preselected login properties; no enchantment registry. Property visibility policy and registry serializer are not exercised.', 'vectors': json.loads(result)}
    (root.parent / 'fixtures/messages.json').write_text(json.dumps(fixtures, indent=2) + '\n')
