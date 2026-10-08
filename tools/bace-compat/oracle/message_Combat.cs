using System;
using System.IO;
using System.Linq;
using System.Collections.Generic;
using ACE.Entity;
using ACE.Entity.Enum;
using ACE.Server.Network;
using ACE.Server.Network.Enum;
using ACE.Server.Network.GameAction;
using ACE.Server.Network.GameAction.Actions;
using ACE.Server.Network.GameMessages;
using ACE.Server.Network.GameMessages.Messages;
using ACE.Server.Network.GameEvent.Events;

static class CombatHarness {
    static Session Fresh()=>new(){Player=new HarnessPlayer{Guid=new ObjectGuid(0x50000001)},GameEventSequence=42};
    static object Message(GameMessage message)=>new{bytes=Convert.ToHexString(message.Data.ToArray()),group=(uint)message.Group};
    public static object WorldControl(){
        var session=Fresh();var login=new ClientMessage(Convert.FromHexString("B1F700002A000000A1000000A55A"));login.Payload.ReadUInt32();login.Payload.ReadUInt32();
        GameActionLoginComplete.Handle(login,session);
        var force=new ClientMessage(Convert.FromHexString("EAF6000001000080A55A"));ACE.Server.Network.Handlers.ControlHandler.ControlResponse(force,session);
        return new {login_complete=new{bytes="B1F700002A000000A1000000A55A",trailing_bytes=login.Data.Length-login.Data.Position,completed=session.Player.FirstEnterWorldDone},force_description=new{bytes="EAF6000001000080A55A",trailing_bytes=force.Data.Length-force.Data.Position,object_id=session.Player.ForcedDescription}};
    }
    public static object Run(){
        var actions=new[]{GameActionType.TargetedMeleeAttack,GameActionType.ChangeCombatMode,GameActionType.CancelAttack,GameActionType.QueryHealth}.Select(kind=>{
            using var ms=new MemoryStream();using var writer=new BinaryWriter(ms);writer.Write((uint)GameMessageOpcode.GameAction);writer.Write(42u);writer.Write((uint)kind);
            switch(kind){
                case GameActionType.TargetedMeleeAttack:writer.Write(0x80000001u);writer.Write(2u);writer.Write(0.35f);break;
                case GameActionType.ChangeCombatMode:writer.Write(2u);break;
                case GameActionType.QueryHealth:writer.Write(0x80000001u);break;
            }
            writer.Write(new byte[]{0xa5,0x5a});var bytes=ms.ToArray();var message=new ClientMessage(bytes);message.Payload.ReadUInt32();message.Payload.ReadUInt32();var session=Fresh();
            switch(kind){
                case GameActionType.TargetedMeleeAttack:GameActionTargetedMeleeAttack.Handle(message,session);break;
                case GameActionType.ChangeCombatMode:GameActionChangeCombatMode.Handle(message,session);break;
                case GameActionType.CancelAttack:GameActionCancelAttack.Handle(message,session);break;
                case GameActionType.QueryHealth:GameActionQueryHealth.Handle(message,session);break;
            }
            return new{bytes=Convert.ToHexString(bytes),captured=session.Player.CombatCapture,consumed=message.Data.Position,trailing_bytes=message.Data.Length-message.Data.Position};
        }).ToArray();
        var events=new Dictionary<string,object>{
            ["done"]=Message(new GameEventAttackDone(Fresh(),(WeenieError)0x36)),
            ["commence"]=Message(new GameEventCombatCommenceAttack(Fresh())),
            ["attacker"]=Message(new GameEventAttackerNotification(Fresh(),"Drudge café",(DamageType)4,0.35f,37,true,unchecked((AttackConditions)0x87654321))),
            ["defender"]=Message(new GameEventDefenderNotification(Fresh(),"Drudge café",(DamageType)4,0.35f,37,(DamageLocation)6,true,unchecked((AttackConditions)0x87654321))),
            ["evaded_by"]=Message(new GameEventEvasionAttackerNotification(Fresh(),"Drudge café")),
            ["evaded_from"]=Message(new GameEventEvasionDefenderNotification(Fresh(),"Drudge café")),
            ["health"]=Message(new GameEventUpdateHealth(Fresh(),0x80000001,0.35f)),
            ["killer"]=Message(new GameEventKillerNotification(Fresh(),"Drudge café")),
            ["victim"]=Message(new GameEventVictimNotification(Fresh(),"Drudge café")),
            ["sound"]=Message(new GameMessageSound(new ObjectGuid(0x80000001),(Sound)7,0.35f)),
            ["script"]=Message(new GameMessageScript(new ObjectGuid(0x80000001),(PlayScript)7,0.35f)),
            ["killed"]=Message(new GameMessagePlayerKilled("Drudge café",new ObjectGuid(0x80000001),new ObjectGuid(0x50000001))),
        };
        return new {actions,events};
    }
}
namespace ACE.Server.WorldObjects {
    public partial class Player {
        public bool FirstEnterWorldDone;public uint ForcedDescription;
        public void OnTeleportComplete(){}public void SendPropertyUpdatesAndOverrides(){}
        public void HandleActionForceObjDescSend(uint id){ForcedDescription=id;}
        public object CombatCapture;
        public void HandleActionTargetedMeleeAttack(uint target,uint height,float power){CombatCapture=new{target_id=target,height,power_bits=BitConverter.SingleToUInt32Bits(power)};}
        public void HandleActionChangeCombatMode(CombatMode mode){CombatCapture=new{mode=(uint)mode};}
        public void HandleActionCancelAttack(){CombatCapture=new{empty=true};}
        public void HandleActionQueryHealth(uint target){CombatCapture=new{target_id=target};}
    }
}
