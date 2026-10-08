// Synthetic immutable inputs. Original PlayerDescription handles every output byte.
// Selection of SendOnLogin properties and enchantment registry behavior are excluded.
using System;
using System.IO;
using System.Collections.Generic;
using ACE.Entity;
using ACE.Entity.Enum;
using ACE.Entity.Enum.Properties;
using ACE.Entity.Models;
using ACE.Database.Models.Shard;
using ACE.Server.WorldObjects;
using ACE.Server.WorldObjects.Entity;
using ACE.Server.Network;
using ACE.Server.Network.Structure;
using ACE.Server.Network.GameEvent.Events;
static class PlayerDescriptionHarness {
    public static object LoginOrder(){
        var p=new HarnessPlayer {Guid=new ObjectGuid(0x50000001)};var s=new Session {Player=p,GameEventSequence=42};p.Session=s;
        p.Inventory.Add(1,new Container {Guid=new ObjectGuid(0x80000001)});p.EquippedObjects.Add(2,new WorldObject {Guid=new ObjectGuid(0x80000002)});
        p.OracleSendSelf();
        return s.Network.Messages.ConvertAll(m=>new {opcode=(uint)m.Opcode,group=(uint)m.Group,event_sequence=m is ACE.Server.Network.GameEvent.GameEventMessage ? (uint?)BitConverter.ToUInt32(m.Data.ToArray(),8):null});
    }

    public static object Run(){
        var vectors=new Dictionary<string,object>();
        foreach(var name in new[]{"fresh","properties","position","skills_spells","options","inventory","all","plussed","cloaked"}){
            var p=new HarnessPlayer {Guid=new ObjectGuid(0x50000001),WeenieType=(WeenieType)10};
            p.Strength.StartingValue=10;p.Endurance.StartingValue=20;p.Quickness.StartingValue=30;p.Coordination.StartingValue=40;p.Focus.StartingValue=50;p.Self.StartingValue=60;
            p.Health.Current=70;p.Stamina.Current=80;p.Mana.Current=90;
            var s=new Session {Player=p,GameEventSequence=42};
            if(name=="properties"||name=="all"||name=="plussed"||name=="cloaked"){
                p.LoginInts.Add((PropertyInt)65,-7);p.LoginInts.Add((PropertyInt)1,42);p.LoginInts.Add((PropertyInt)64,0);
                p.LoginInt64.Add((PropertyInt64)1,-1234567890123L);p.LoginBools.Add((PropertyBool)33,true);p.LoginBools.Add((PropertyBool)1,false);
                p.LoginDoubles.Add((PropertyFloat)2,0.35);p.LoginStrings.Add(PropertyString.Name,"Élodie");p.LoginDids.Add((PropertyDataId)1,0x02000001);p.LoginIids.Add((PropertyInstanceId)2,0x50000002);
            }
            if(name=="plussed"||name=="cloaked"){p.IsPlussed=true;if(name=="cloaked")p.CloakStatus=CloakStatus.Player;}
            if(name=="position"||name=="all")p.DeathPosition=MovementHarness.SyntheticPosition();
            if(name=="skills_spells"||name=="all"){
                foreach(var id in new[]{33,1,32})p.Skills.Add((Skill)id,new CreatureSkill{Ranks=(ushort)(id+1),AdvancementClass=(SkillAdvancementClass)2,ExperienceSpent=900,InitLevel=5});
                foreach(var id in new[]{65,1,64})p.Biota.Spells.Add(id,0.25f);
            }
            if(name=="options"||name=="all"){
                p.Character.CharacterOptions1=0x12345678;p.Character.CharacterOptions2=0x87654321;p.Character.SpellbookFilters=0x11223344;
                p.Shortcuts.Add(new Shortcut{Index=2,ObjectId=0x80000001,Spell=new LayeredSpell(7,3)});
                p.SpellBars[0].Add(new CharacterPropertiesSpellBar{SpellId=9});p.SpellBars[7].Add(new CharacterPropertiesSpellBar{SpellId=10});
                foreach(var id in new uint[]{257,1,256})p.Character.Fill.Add(new CharacterPropertiesFillCompBook{SpellComponentId=id,QuantityToRebuy=id+1});
                p.Character.GameplayOptions=new byte[]{1,2,3,4,5,6,7,8};
            }
            if(name=="inventory"||name=="all"){
                p.Inventory.Add(1,new WorldObject{Guid=new ObjectGuid(0x80000001),PlacementPosition=3});
                p.Inventory.Add(2,new WorldObject{Guid=new ObjectGuid(0x80000002),PlacementPosition=1});
                p.Inventory.Add(3,new WorldObject{Guid=new ObjectGuid(0x80000003),PlacementPosition=1,UseBackpackSlot=true,WeenieType=WeenieType.Container});
                p.Inventory.Add(4,new WorldObject{Guid=new ObjectGuid(0x80000004),PlacementPosition=0,UseBackpackSlot=true,WeenieType=(WeenieType)10});
                p.EquippedObjects.Add(1,new WorldObject{Guid=new ObjectGuid(0x80000005),CurrentWieldedLocation=4,ClothingPriority=8});
            }
            var message=new GameEventPlayerDescription(s);vectors[name]=new{bytes=Convert.ToHexString(message.Data.ToArray()),group=(uint)message.Group};
        }
        var titleSession=new Session {Player=new HarnessPlayer {Guid=new ObjectGuid(0x50000001),CharacterTitleId=7},GameEventSequence=42};
        foreach(var id in new uint[]{7,9})titleSession.Player.Character.Titles.Add(new CharacterPropertiesTitleBook{TitleId=id});
        var titleMessage=new GameEventCharacterTitle(titleSession);
        vectors["titles"]=new{bytes=Convert.ToHexString(titleMessage.Data.ToArray()),group=(uint)titleMessage.Group};
        return vectors;
    }
}
namespace ACE.Entity.Enum.Properties {
    public enum PositionType { LastOutsideDeath=14 }
    public static class SendOnLoginProperties {public static int PropertiesInt,PropertiesInt64,PropertiesBool,PropertiesDouble,PropertiesString,PropertiesDataId,PropertiesInstanceId;}
}
namespace ACE.Entity.Models {public class PropertiesEnchantmentRegistry {public int SpellId;public ushort LayerId;}}
namespace ACE.Database.Models.Shard { public class CharacterPropertiesShortcutBar {public uint ShortcutBarIndex,ShortcutObjectId;} }
namespace ACE.Server.WorldObjects {
    public partial class WorldObject {public bool UseBackpackSlot;public Player Wielder;}
    public sealed class LoginBiota {public Dictionary<int,float> Spells=new();public Dictionary<int,float> CloneSpells(object gate)=>Spells;}
    public sealed class LoginEnchantments {public bool HasEnchantments=>false;public void SendRegistry(BinaryWriter w)=>throw new NotSupportedException("Enchantment registry not exercised");}
    public sealed partial class HarnessCharacter {
        public uint CharacterOptions1,CharacterOptions2,SpellbookFilters;public byte[] GameplayOptions;
        public List<CharacterPropertiesTitleBook> Titles=new();public List<CharacterPropertiesTitleBook> GetTitles(object gate)=>Titles;
        public List<CharacterPropertiesFillCompBook> Fill=new();public List<CharacterPropertiesFillCompBook> GetFillComponents(object gate)=>Fill;
    }
    public partial class Player {
        public void OracleSendSelf()=>SendSelf();public void SendContractTrackerTable(){}
        public bool IsPlussed;public int? CharacterTitleId,NumCharacterTitles;
        public Dictionary<PropertyInt,int> LoginInts=new();public Dictionary<PropertyInt64,long> LoginInt64=new();public Dictionary<PropertyBool,bool> LoginBools=new();public Dictionary<PropertyFloat,double> LoginDoubles=new();public Dictionary<PropertyString,string> LoginStrings=new();public Dictionary<PropertyDataId,uint> LoginDids=new();public Dictionary<PropertyInstanceId,uint> LoginIids=new();
        public Dictionary<PropertyInt,int> GetAllPropertyIntWhere(int filter)=>LoginInts;
        public Dictionary<PropertyInt64,long> GetAllPropertyInt64Where(int filter)=>LoginInt64;
        public Dictionary<PropertyBool,bool> GetAllPropertyBoolsWhere(int filter)=>LoginBools;
        public Dictionary<PropertyFloat,double> GetAllPropertyFloatWhere(int filter)=>LoginDoubles;
        public Dictionary<PropertyString,string> GetAllPropertyStringWhere(int filter)=>LoginStrings;
        public Dictionary<PropertyDataId,uint> GetAllPropertyDataIdWhere(int filter)=>LoginDids;
        public Dictionary<PropertyInstanceId,uint> GetAllPropertyInstanceIdWhere(int filter)=>LoginIids;
        public Position DeathPosition;public Position GetPosition(PositionType kind)=>DeathPosition;
        public CreatureAttribute Strength=new(),Endurance=new(),Quickness=new(),Coordination=new(),Focus=new(),Self=new();
        public CreatureVital Health=new(),Stamina=new(),Mana=new();
        public Dictionary<Skill,CreatureSkill> Skills=new();public LoginBiota Biota=new();public object BiotaDatabaseLock=new();public LoginEnchantments EnchantmentManager=new();
        public List<Shortcut> Shortcuts=new();public List<Shortcut> GetShortcuts()=>Shortcuts;
        public List<CharacterPropertiesSpellBar>[] SpellBars=new List<CharacterPropertiesSpellBar>[]{new(),new(),new(),new(),new(),new(),new(),new()};public List<CharacterPropertiesSpellBar> GetSpellsInSpellBar(int i)=>SpellBars[i];
        public Dictionary<uint,WorldObject> Inventory=new(),EquippedObjects=new();
    }
}

namespace ACE.Server.Network {
    public sealed class LoginNetwork {public List<ACE.Server.Network.GameMessages.GameMessage> Messages=new();public void EnqueueSend(params ACE.Server.Network.GameMessages.GameMessage[] messages)=>Messages.AddRange(messages);}
    public sealed partial class Session {public LoginNetwork Network=new();}
}
