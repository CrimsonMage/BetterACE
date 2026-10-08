using System;using System.Collections.Generic;
static class Extensions {public static bool EpsilonEquals(this float a,float b)=>Math.Abs(a-b)<0.0001f;}
class Entry{public float StatModValue;}
static class PropertyManager{public static double Max=0.4;public static (double Item,int x) GetDouble(string n)=>(Max,0);}
class EnchantmentManager{public Entry Value=new();public bool Removed;public Entry GetVitae()=>Value;public void RemoveVitae(){Removed=true;}public void SendUpdateVitae(){}
 // MIN
 // REDUCE
}
class Logger{public void Error(string s){}}
class SessionType{public Network Network=new();}class Network{public void EnqueueSend(params object[] values){}}
enum PropertyInt{VitaeCpPool}enum ChatMessageType{Magic}
class GameMessagePrivateUpdatePropertyInt{public GameMessagePrivateUpdatePropertyInt(params object[] args){}}class GameMessageSystemChat{public GameMessageSystemChat(params object[] args){}}
class ActionChain{public static List<Action> Pending=new();Action a;public void AddDelaySeconds(float delay){}public void AddAction(Player p,Action a){this.a=a;}public void EnqueueChain(){Pending.Add(a);}}
class Player{
 public EnchantmentManager EnchantmentManager=new();public int? DeathLevel=100,VitaeCpPool=0;public SessionType Session=new();Logger log=new();string Name="fixture";
 // THRESHOLD
 // UPDATE
 public void Run(long amount){UpdateXpVitae(amount);}
}
class Program{static void Main(){foreach(var level in new[]{1,10,100,275})foreach(var value in new[]{0.6f,0.925f,0.95f,0.99f,1.0f})foreach(var pool in new[]{0,100})foreach(long amount in new long[]{0,20,1000,10000000}){
 var p=new Player{DeathLevel=level,VitaeCpPool=pool};p.EnchantmentManager.Value.StatModValue=value;ActionChain.Pending.Clear();p.Run(amount);Console.WriteLine($"{level},{BitConverter.SingleToUInt32Bits(value)},{pool},{amount},{p.VitaeCpPool},{BitConverter.SingleToUInt32Bits(p.EnchantmentManager.Value.StatModValue)},{ActionChain.Pending.Count}");
 }}}
