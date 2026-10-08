// Deterministic source adapters. AGPL-3.0-only.
using ACE.Entity.Enum;
using System.Globalization;
public class ObjectGuid {public uint Full;public ObjectGuid(uint id){Full=id;}public bool IsPlayer()=>Full>=0x50000001&&Full<=0x5fffffff;}
public partial class Corpse {public uint? KillerId;}
public static class ThreadSafeRandom {public static int Draw;public static int Next(int min,int max){if(Draw<min||Draw>max)throw new Exception();return Draw;}}
public static class PropertyManager {public static double Maximum;public static (double Item,string Description) GetDouble(string key)=>(Maximum,"");}
public partial class Player {public int? Level;public int? CoinValue;public int AugmentationLessDeathItemLoss;public const int MaxItemsDropped=14;public ObjectGuid Guid=new(0x50000001);public PlayerKillerStatus PlayerKillerStatus;}
public readonly record struct LandblockId(int LandblockX,int LandblockY);
public partial class Position {public LandblockId LandblockId;public float PositionX,PositionY,PositionZ;public Position(int lb,float x,float y,float z){LandblockId=new(lb>>8,lb&255);PositionX=x;PositionY=y;PositionZ=z;}}
class Program {static void Main(){CultureInfo.CurrentCulture=CultureInfo.InvariantCulture;
 foreach(var id in Player.NoLog_Landblocks)Console.WriteLine($"L\t{id}");
 foreach(var id in Corpse.NoDrop_Landblocks)Console.WriteLine($"D\t{id}");
 foreach(var id in Creature.NoDeathXP_Landblocks)Console.WriteLine($"X\t{id}");
 foreach(var max in new[]{0.0,0.25,0.40,0.90}){PropertyManager.Maximum=max;for(uint level=1;level<=275;level++)Console.WriteLine($"V\t{level}\t{max:R}\t{new EnchantmentManager().GetMinVitae(level):R}");}
 foreach(var pk in new[]{false,true})for(int aug=0;aug<=3;aug++)for(int level=1;level<=275;level++)for(int draw=0;draw<=(level<=20?1:2);draw++){
 var p=new Player{Level=level,AugmentationLessDeathItemLoss=aug,PlayerKillerStatus=pk?PlayerKillerStatus.PK:PlayerKillerStatus.NPK};ThreadSafeRandom.Draw=draw;Console.WriteLine($"I\t{level}\t{aug}\t{(pk?1:0)}\t{draw}\t{p.GetNumItemsDropped(new Corpse{KillerId=0x50000002})}");}
 foreach(uint id in new uint[]{0,0x50000000,0x50000001,0x50000002,0x5fffffff,0x60000000})for(uint status=0;status<=127;status++){
 var p=new Player{PlayerKillerStatus=(PlayerKillerStatus)status};Console.WriteLine($"K\t{status}\t{id}\t{(p.IsPKDeath(id)?1:p.IsPKLiteDeath(id)?2:0)}");}
 foreach(float x in new[]{0f,7.9999995f,8f,8.000001f,192f})foreach(int block in new[]{0,1,256,65535}){
 var a=new Position(0,0,0,0);var b=new Position(block,x,0,0);Console.WriteLine($"M\t{block}\t{x:R}\t{(a.SquaredDistanceTo(b)>64?1:0)}");}
}}
