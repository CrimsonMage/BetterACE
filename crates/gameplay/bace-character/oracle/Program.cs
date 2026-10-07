// Harness-only adapters around verbatim official ACE method bodies. The
// generated program is temporary; upstream attribution remains in provenance.
using System;
using System.Collections.Generic;

enum SkillAdvancementClass { Inactive, Untrained, Trained, Specialized }
enum Skill { Synthetic }
static class SkillExtensions { public static string ToSentence(this Skill skill) => "synthetic"; }
enum PropertyInt { AvailableSkillCredits }
enum ChatMessageType { Advancement }
class GameMessagePrivateUpdateSkill { public GameMessagePrivateUpdateSkill(Player player, CreatureSkill skill) { } }
class GameMessagePrivateUpdatePropertyInt { public GameMessagePrivateUpdatePropertyInt(Player player, PropertyInt property, int value) { } }
class GameMessageSystemChat { public GameMessageSystemChat(string text, ChatMessageType channel) { } }
enum CreateResult { Success, TooManySkillCreditsUsed, InvalidSkillRequested }
class CharacterCreateInfo
{
    public uint StrengthAbility, EnduranceAbility, CoordinationAbility, QuicknessAbility, FocusAbility, SelfAbility;
}
enum PropertyInt64 { AvailableExperience }
class GameMessagePrivateUpdatePropertyInt64
{
    public GameMessagePrivateUpdatePropertyInt64(Player player, PropertyInt64 property, long value) { }
}
class Network { public void EnqueueSend(params object[] messages) { } }
class Session { public Network Network = new(); }
class Log { public void Warn(string message) { } }
class XpTable
{
    public List<uint> AttributeXpList = new() { 0, 10, 30, 30, 100 };
    public List<uint> VitalXpList = new() { 0, 2, 8, 20, 100 };
    public List<uint> TrainedSkillXpList = new() { 0, 5, 15, 50, 100 };
    public List<uint> SpecializedSkillXpList = new() { 0, 1, 7, 40, 100 };
}
class SkillBase { public int TrainedCost; }
class SkillTable { public Dictionary<uint, SkillBase> SkillBaseHash = new(); }
class PortalDat { public XpTable XpTable = new(); public SkillTable SkillTable = new(); }
static class DatManager { public static PortalDat PortalDat = new(); }
class Trait
{
    public List<uint> Table;
    public uint ExperienceSpent;
    public ushort Ranks;
    public uint ExperienceLeft => Table[Table.Count - 1] - ExperienceSpent;
    public bool IsMaxRank => Ranks >= Table.Count - 1;
}
class CreatureAttribute : Trait { public string Attribute = "synthetic"; }
class CreatureVital : Trait { public string Vital = "synthetic"; }
class CreatureSkill : Trait
{
    public string Skill = "synthetic";
    public SkillAdvancementClass AdvancementClass;
    public uint InitLevel;
}
class Player
{
    public long? AvailableExperience;
    public string Name = "synthetic";
    public Session Session = new();
    private Log log = new();
    public int? AvailableSkillCredits;
    public CreatureSkill CreationSkill = new() { AdvancementClass = SkillAdvancementClass.Untrained };
    private CreatureSkill GetCreatureSkill(Skill skill) => CreationSkill;
    private bool IsSkillSpecializedViaAugmentation(Skill skill, out bool hasAugmentation)
    { hasAugmentation = false; return false; }
    // OFFICIAL_METHODS

    public static CreateResult Attributes(CharacterCreateInfo info, uint maximum) => ValidateAttributeCredits(info, maximum);

    public bool Spend(int kind, Trait trait, uint amount) => kind switch
    {
        0 => SpendAttributeXp((CreatureAttribute)trait, amount),
        1 => SpendVitalXp((CreatureVital)trait, amount),
        _ => SpendSkillXp((CreatureSkill)trait, amount),
    };
}
class Program
{
    static void Main()
    {
        for (int kind = 0; kind < 4; kind++)
        foreach (uint spent in new uint[] { 0, 1, 9, 10, 29, 30, 99, 100 })
        foreach (long available in new long[] { 0, 5, 100, 4_294_967_296 })
        foreach (uint amount in new uint[] { 0, 1, 2, 9, 10, 20, 30, 99, 100, 101, uint.MaxValue })
        {
            var tables = DatManager.PortalDat.XpTable;
            Trait trait = kind switch
            {
                0 => new CreatureAttribute { Table = tables.AttributeXpList },
                1 => new CreatureVital { Table = tables.VitalXpList },
                2 => new CreatureSkill { Table = tables.TrainedSkillXpList, AdvancementClass = SkillAdvancementClass.Trained },
                _ => new CreatureSkill { Table = tables.SpecializedSkillXpList, AdvancementClass = SkillAdvancementClass.Specialized },
            };
            trait.ExperienceSpent = spent;
            trait.Ranks = (ushort)(kind switch
            {
                0 => Player.CalcAttributeRank(spent),
                1 => Player.CalcVitalRank(spent),
                2 => Player.CalcSkillRank(SkillAdvancementClass.Trained, spent),
                _ => Player.CalcSkillRank(SkillAdvancementClass.Specialized, spent),
            });
            var player = new Player { AvailableExperience = available };
            bool accepted = player.Spend(kind, trait, amount);
            Console.WriteLine($"{kind},{spent},{available},{amount},{(accepted ? 1 : 0)},{trait.ExperienceSpent},{trait.Ranks},{player.AvailableExperience}");
        }
        foreach (uint strength in new uint[] { 0, 9, 10, 11, 99, 100, 101, uint.MaxValue })
        foreach (uint other in new uint[] { 10, 50, 100 })
        foreach (uint budget in new uint[] { 0, 60, 330, 600, uint.MaxValue })
        {
            var info = new CharacterCreateInfo {
                StrengthAbility = strength, EnduranceAbility = other, CoordinationAbility = other,
                QuicknessAbility = other, FocusAbility = other, SelfAbility = other,
            };
            Console.WriteLine($"creation,attr,{strength},{other},{other},{other},{other},{other},{budget},{(int)Player.Attributes(info, budget)}");
        }
        foreach (int advancement in new int[] { 2, 3 })
        foreach (int credits in new int[] { 0, 5, 10, 52 })
        foreach (int trained in new int[] { 0, 2, 5, 10, 53 })
        foreach (int specialized in new int[] { 0, 2, 5, 10, 53 })
        {
            var player = new Player { AvailableSkillCredits = credits };
            bool success = player.TrainSkill(Skill.Synthetic, trained, advancement == 2);
            if (success && advancement == 3)
                success = player.SpecializeSkill(Skill.Synthetic, specialized);
            var skill = player.CreationSkill;
            Console.WriteLine($"creation,skill,{advancement},{credits},{trained},{specialized},{(success ? 1 : 0)},{player.AvailableSkillCredits},{skill.ExperienceSpent},{skill.Ranks},{skill.InitLevel}");
        }
        foreach (int advancement in new int[] { 0, 1, 2, 3 })
        foreach (int credits in new int[] { 0, 4, 20 })
        foreach (int price in new int[] { 0, 4, 20 })
        foreach (int quote in new int[] { -1, 0, 4, 20, 21 })
        {
            uint xp = advancement < 2 ? 0u : 9u;
            var player = new Player { AvailableSkillCredits = credits };
            player.CreationSkill = new() { AdvancementClass = (SkillAdvancementClass)advancement, ExperienceSpent = xp, InitLevel = 123,
                Ranks = advancement < 2 ? (ushort)0 : (ushort)Player.CalcSkillRank((SkillAdvancementClass)advancement, xp) };
            DatManager.PortalDat.SkillTable.SkillBaseHash[0] = new() { TrainedCost = price };
            bool success = player.HandleActionTrainSkill(Skill.Synthetic, quote);
            var skill = player.CreationSkill;
            Console.WriteLine($"training,train,{advancement},{credits},{price},{quote},{xp},123,{(success ? 1 : 0)},{(int)skill.AdvancementClass},{skill.ExperienceSpent},{skill.Ranks},{skill.InitLevel},{player.AvailableSkillCredits}");
        }
        foreach (int advancement in new int[] { 0, 1, 2, 3 })
        foreach (int credits in new int[] { 0, 4, 20 })
        foreach (int price in new int[] { 0, 4, 20 })
        foreach (uint candidateXp in new uint[] { 0, 9, 30, 100 })
        {
            if (advancement < 2 && candidateXp != 0) continue;
            var player = new Player { AvailableSkillCredits = credits };
            player.CreationSkill = new() { AdvancementClass = (SkillAdvancementClass)advancement, ExperienceSpent = candidateXp, InitLevel = 123,
                Ranks = advancement < 2 ? (ushort)0 : (ushort)Player.CalcSkillRank((SkillAdvancementClass)advancement, candidateXp) };
            bool success = player.SpecializeSkill(Skill.Synthetic, price, false);
            var skill = player.CreationSkill;
            Console.WriteLine($"training,specialize,{advancement},{credits},{price},{price},{candidateXp},123,{(success ? 1 : 0)},{(int)skill.AdvancementClass},{skill.ExperienceSpent},{skill.Ranks},{skill.InitLevel},{player.AvailableSkillCredits}");
        }
    }
}
