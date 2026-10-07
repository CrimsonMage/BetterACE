using System;
using System.Globalization;

namespace Database.Models.World
{
    class Quest { public string Name; public uint MinDelta; public int MaxSolves; }
}
class Record { public uint LastTimeCompleted; public int NumTimesCompleted; }
class World
{
    public Database.Models.World.Quest Definition;
    public Database.Models.World.Quest GetCachedQuest(string name) => Definition;
}
static class DatabaseManager { public static World World = new(); }
static class Time { public static uint Now; public static double GetUnixTime() => Now; }
static class PropertyManager
{
    public static double Rate;
    public static (double Item, string Description) GetDouble(string name, double fallback) => (Rate, "synthetic");
}
class QuestManager
{
    public bool Debug = false;
    public string Name = "synthetic";
    public Record Progress;
    public Record GetQuest(string name) => Progress;
    // OFFICIAL_METHODS
}
class Program
{
    static void Main()
    {
        CultureInfo.CurrentCulture = CultureInfo.InvariantCulture;
        foreach (int exists in new int[] { 0, 1 })
        foreach (int hasProgress in new int[] { 0, 1 })
        foreach (string name in new string[] { "Quest", "ColoArenaTrial", "coloarenaTrial" })
        foreach (int maximum in new int[] { -1, 0, 1, 2 })
        foreach (int solves in new int[] { 0, 1, 2 })
        foreach (double rate in new double[] { 0, 0.5, 1, 1.25, 2 })
        foreach (uint now in new uint[] { 99, 100, 102, 105, 110, 120 })
        {
            DatabaseManager.World.Definition = exists == 1 ? new() { Name = name, MinDelta = 5, MaxSolves = maximum } : null;
            var manager = new QuestManager { Progress = hasProgress == 1 ? new() { LastTimeCompleted = 100, NumTimesCompleted = solves } : null };
            Time.Now = now;
            PropertyManager.Rate = rate;
            var wait = manager.GetNextSolveTime(name);
            string result = wait == TimeSpan.MinValue ? "ready" : wait == TimeSpan.MaxValue ? "blocked" : ((uint)wait.TotalSeconds).ToString();
            Console.WriteLine($"time,{exists},{hasProgress},{name},{maximum},{solves},{rate},{now},{result}");
        }
        foreach (int exists in new int[] { 0, 1 })
        foreach (int solves in new int[] { int.MinValue, -1, 0, 1, 2, 3, int.MaxValue })
        foreach (int bits in new int[] { int.MinValue, -1, 0, 1, 2, 3, int.MaxValue })
        {
            var manager = new QuestManager { Progress = exists == 1 ? new() { NumTimesCompleted = solves } : null };
            Console.WriteLine($"bits,{exists},{solves},{bits},{manager.HasQuestBits("synthetic", bits)},{manager.HasNoQuestBits("synthetic", bits)}");
        }
        foreach (int exists in new int[] { 0, 1 })
        foreach (int solves in new int[] { int.MinValue, -1, 0, 1, 2, int.MaxValue })
        foreach (int? minimum in new int?[] { null, -1, 0, 2 })
        foreach (int? maximum in new int?[] { null, -1, 0, 2 })
        {
            var manager = new QuestManager { Progress = exists == 1 ? new() { NumTimesCompleted = solves } : null };
            Console.WriteLine($"range,{exists},{solves},{minimum?.ToString() ?? "none"},{maximum?.ToString() ?? "none"},{manager.HasQuestSolves("synthetic", minimum, maximum)}");
        }
    }
}
