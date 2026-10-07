using System;
using System.IO;
using System.Linq;
using System.Collections.Generic;
using ACE.Entity;
using ACE.Entity.Enum;
using ACE.Server.Entity;
using ACE.Server.Network;
using ACE.Server.Network.GameAction;
using ACE.Server.Network.GameAction.Actions;
using ACE.Server.Network.GameMessages;
using ACE.Server.Network.GameEvent.Events;
using ACE.Server.WorldObjects;
using ACE.Server.Managers;

static class InventoryHarness {
    static Session Fresh()=>new(){Player=new HarnessPlayer{Guid=new ObjectGuid(0x50000001)},GameEventSequence=42};
    static object Message(GameMessage message)=>new{bytes=Convert.ToHexString(message.Data.ToArray()),group=(uint)message.Group};
    public static object Run(){
        var kinds=new[]{GameActionType.PutItemInContainer,GameActionType.GetAndWieldItem,GameActionType.DropItem,GameActionType.Use,GameActionType.UseWithTarget,GameActionType.StackableMerge,GameActionType.StackableSplitToContainer,GameActionType.StackableSplitTo3D,GameActionType.StackableSplitToWield,GameActionType.GiveObjectRequest,GameActionType.Buy,GameActionType.Sell,GameActionType.OpenTradeNegotiations,GameActionType.CloseTradeNegotiations,GameActionType.AddToTrade,GameActionType.AcceptTrade,GameActionType.DeclineTrade,GameActionType.ResetTrade,GameActionType.NoLongerViewingContents};
        var actions=kinds.Select(kind=>{
            using var ms=new MemoryStream();using var writer=new BinaryWriter(ms);writer.Write((uint)GameMessageOpcode.GameAction);writer.Write(42u);writer.Write((uint)kind);
            switch(kind){
                case GameActionType.PutItemInContainer:writer.Write(0x80000001u);writer.Write(0x50000002u);writer.Write(-2);break;
                case GameActionType.GetAndWieldItem:writer.Write(0x80000001u);writer.Write(0x87654321u);break;
                case GameActionType.DropItem:case GameActionType.Use:case GameActionType.NoLongerViewingContents:writer.Write(0x80000001u);break;
                case GameActionType.UseWithTarget:writer.Write(0x80000001u);writer.Write(0x50000002u);break;
                case GameActionType.StackableMerge:writer.Write(0x80000001u);writer.Write(0x50000002u);writer.Write(-3);break;
                case GameActionType.StackableSplitToContainer:writer.Write(0x80000001u);writer.Write(0x50000002u);writer.Write(-2);writer.Write(-3);break;
                case GameActionType.StackableSplitTo3D:writer.Write(0x80000001u);writer.Write(-3);break;
                case GameActionType.StackableSplitToWield:writer.Write(0x80000001u);writer.Write(0x87654321u);writer.Write(-3);break;
                case GameActionType.GiveObjectRequest:writer.Write(0x50000002u);writer.Write(0x80000001u);writer.Write(-3);break;
                case GameActionType.Buy:case GameActionType.Sell:writer.Write(0x50000002u);writer.Write(2u);writer.Write(-3);writer.Write(0x80000001u);writer.Write(5);writer.Write(0x80000002u);writer.Write(0x87654321u);break;
                case GameActionType.OpenTradeNegotiations:writer.Write(0x50000002u);break;
                case GameActionType.AddToTrade:writer.Write(0x80000001u);writer.Write(7u);break;
                case GameActionType.AcceptTrade:writer.Write(0x50000002u);writer.Write(12.25);writer.Write(0x12345678u);writer.Write(0x50000001u);writer.Write(2u);writer.Write(0u);break;
            }
            writer.Write(new byte[]{0xa5,0x5a});var bytes=ms.ToArray();var message=new ClientMessage(bytes);message.Payload.ReadUInt32();message.Payload.ReadUInt32();var session=Fresh();object captured=null;
            switch(kind){
                case GameActionType.PutItemInContainer:GameActionPutItemInContainer.Handle(message,session);break;
                case GameActionType.GetAndWieldItem:GameActionGetAndWieldItem.Handle(message,session);break;
                case GameActionType.DropItem:GameActionDropItem.Handle(message,session);break;
                case GameActionType.Use:GameActionUseItem.Handle(message,session);break;
                case GameActionType.UseWithTarget:GameActionUseWithTarget.Handle(message,session);break;
                case GameActionType.StackableMerge:GameActionStackableMerge.Handle(message,session);break;
                case GameActionType.StackableSplitToContainer:GameActionStackableSplitToContainer.Handle(message,session);break;
                case GameActionType.StackableSplitTo3D:GameActionStackableSplitTo3D.Handle(message,session);break;
                case GameActionType.StackableSplitToWield:GameActionStackableSplitToWield.Handle(message,session);break;
                case GameActionType.GiveObjectRequest:GameActionGiveObjectRequest.Handle(message,session);break;
                case GameActionType.Buy:GameActionBuyItems.Handle(message,session);break;
                case GameActionType.Sell:GameActionSellItems.Handle(message,session);break;
                case GameActionType.OpenTradeNegotiations:GameActionOpenTradeNegotiations.Handle(message,session);break;
                case GameActionType.CloseTradeNegotiations:GameActionCloseTradeNegotiations.Handle(message,session);break;
                case GameActionType.AddToTrade:GameActionAddToTrade.Handle(message,session);break;
                case GameActionType.AcceptTrade:captured=ExtractedAcceptTrade.Read(message);break;
                case GameActionType.DeclineTrade:GameActionDeclineTrade.Handle(message,session);break;
                case GameActionType.ResetTrade:GameActionResetTrade.Handle(message,session);break;
                case GameActionType.NoLongerViewingContents:GameActionNoLongerViewingContents.Handle(message,session);break;
            }
            return new{bytes=Convert.ToHexString(bytes),captured=captured??session.Player.InventoryCapture??new{empty=true},consumed=message.Data.Position,trailing_bytes=message.Data.Length-message.Data.Position};
        }).ToArray();
        var container=new Container{Guid=new ObjectGuid(0x50000010)};
        var first=new WorldObject{Guid=new ObjectGuid(0x80000001),PlacementPosition=3,ContainerType=ContainerType.NonContainer};
        container.Inventory.Add(1,first);container.Inventory.Add(3,new WorldObject{Guid=new ObjectGuid(0x80000003),PlacementPosition=1,RequiresPackSlot=true});container.Inventory.Add(2,new WorldObject{Guid=new ObjectGuid(0x80000002),PlacementPosition=1,WeenieType=WeenieType.Container});
        var events=new Dictionary<string,object>{
            ["put_world"]=Message(new GameEventItemServerSaysMoveItem(Fresh(),first)),["put_container"]=Message(new GameEventItemServerSaysContainId(Fresh(),first,container)),
            ["wield"]=Message(new GameEventWieldItem(Fresh(),0x80000001,(EquipMask)0x87654321)),["view"]=Message(new GameEventViewContents(Fresh(),container)),["close_container"]=Message(new GameEventCloseGroundContainer(Fresh(),container)),
            ["save_failed"]=Message(new GameEventInventoryServerSaveFailed(Fresh(),0x80000001,(WeenieError)0x36)),
            ["register_trade"]=Message(new GameEventRegisterTrade(Fresh(),new ObjectGuid(0x50000001),new ObjectGuid(0x50000002))),
            ["add_trade"]=Message(new GameEventAddToTrade(Fresh(),0x80000001,TradeSide.Partner)),["accept_trade"]=Message(new GameEventAcceptTrade(Fresh(),new ObjectGuid(0x50000002))),
            ["decline_trade"]=Message(new GameEventDeclineTrade(Fresh(),new ObjectGuid(0x50000002))),["reset_trade"]=Message(new GameEventResetTrade(Fresh(),new ObjectGuid(0x50000002))),
            ["close_trade"]=Message(new GameEventCloseTrade(Fresh(),EndTradeReason.Canceled)),["clear_acceptance"]=Message(new GameEventClearTradeAcceptance(Fresh())),
            ["trade_failure"]=Message(new GameEventTradeFailure(Fresh(),0x80000001,(WeenieError)0x36)),
        };
        var vendors=new[]{false,true}.Select(alternate=>{
            var vendor=new Vendor{Guid=new ObjectGuid(0x50000020),MerchandiseItemTypes=0x12345678,MerchandiseMinValue=1,MerchandiseMaxValue=999999,DealMagicalItems=true,BuyPrice=.75,SellPrice=1.25,AlternateCurrency=alternate?123u:null};
            var first=ObjectHarness.Input();first.Guid=new ObjectGuid(0x80000031);first.Header=(WeenieHeaderFlag)uint.MaxValue;first.Header2=(WeenieHeaderFlag2)15;first.ObjectDescriptionFlags|=ObjectDescriptionFlag.IncludesSecondHeader;first.VendorShopCreateListStackSize=-1;
            var second=ObjectHarness.Input();second.Guid=new ObjectGuid(0x80000032);second.Header=(WeenieHeaderFlag)uint.MaxValue;second.Header2=(WeenieHeaderFlag2)15;second.ObjectDescriptionFlags|=ObjectDescriptionFlag.IncludesSecondHeader;second.VendorShopCreateListStackSize=5;
            vendor.DefaultItemsForSale.Add(first.Guid,first);vendor.UniqueItemsForSale.Add(second.Guid,second);
            return new{alternate,message=Message(new GameEventApproachVendor(Fresh(),vendor,7))};
        }).ToArray();
        return new{actions,events,vendors};
    }
}
namespace ACE.Server.WorldObjects {
    public partial class WorldObject{public int? VendorShopCreateListStackSize;public int? PlacementPosition;public WeenieType WeenieType;public bool RequiresPackSlot;public ContainerType ContainerType;}
    public partial class Vendor:Container{
        public uint MerchandiseItemTypes;public int? MerchandiseMinValue,MerchandiseMaxValue;public bool? DealMagicalItems;public double? BuyPrice,SellPrice;public uint? AlternateCurrency;
        public Dictionary<ObjectGuid,WorldObject> DefaultItemsForSale=new(),UniqueItemsForSale=new();
    }
    public class Container:WorldObject{public Dictionary<uint,WorldObject> Inventory=new();}
    public partial class Player{
        public uint TradePartner;
        public void HandleActionCloseTradeNegotiations(){}
        public void HandleActionResetTrade(ObjectGuid guid){}
    }
}
namespace ACE.Server.Network {
    public sealed partial class HarnessPlayer{
        public object InventoryCapture;
        public int GetNumInventoryItemsOfWCID(uint id)=>9;
        public void HandleActionPutItemInContainer(uint item,uint container,int placement){InventoryCapture=new{item_id=item,container_id=container,placement};}
        public void HandleActionGetAndWieldItem(uint item,EquipMask location){InventoryCapture=new{item_id=item,location=(uint)location};}
        public void HandleActionDropItem(uint item){InventoryCapture=new{object_id=item};}
        public void HandleActionUseItem(uint item){InventoryCapture=new{object_id=item};}
        public void HandleActionUseWithTarget(uint source,uint target){InventoryCapture=new{source_id=source,target_id=target};}
        public void HandleActionStackableMerge(uint source,uint target,int amount){InventoryCapture=new{source_id=source,target_id=target,amount};}
        public void HandleActionStackableSplitToContainer(uint stack,uint container,int placement,int amount){InventoryCapture=new{stack_id=stack,container_id=container,placement,amount};}
        public void HandleActionStackableSplitTo3D(uint stack,int amount){InventoryCapture=new{stack_id=stack,amount};}
        public void HandleActionStackableSplitToWield(uint stack,EquipMask location,int amount){InventoryCapture=new{stack_id=stack,location=(uint)location,amount};}
        public void HandleActionGiveObjectRequest(uint target,uint item,int amount){InventoryCapture=new{target_id=target,item_id=item,amount};}
        public void HandleActionBuyItem(uint vendor,List<ItemProfile> items){InventoryCapture=new{vendor_id=vendor,items=items.Select(i=>new{amount=i.Amount,object_id=i.ObjectGuid}).ToArray()};}
        public void HandleActionSellItem(uint vendor,List<ItemProfile> items){HandleActionBuyItem(vendor,items);}
        public void HandleActionOpenTradeNegotiations(uint partner,bool initiator){InventoryCapture=new{object_id=partner};}
        public void HandleActionAddToTrade(uint item,uint slot){InventoryCapture=new{item_id=item,slot};}
        public void HandleActionAcceptTrade(){}
        public void HandleActionDeclineTrade(Session session){}
        public void HandleActionNoLongerViewingContents(uint item){InventoryCapture=new{object_id=item};}
    }
}

namespace ACE.Database{
    public static class DatabaseManager{public static HarnessWorld World=new();}
    public sealed class HarnessWorld{public HarnessCurrency GetCachedWeenie(uint id)=>new();}
    public sealed class HarnessCurrency{public string GetPluralName()=>"Trade tokens";}
}
