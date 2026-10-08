using ACE.Entity.Enum;
foreach(var school in new[]{1,2,3,4,5}) foreach(var category in new[]{0,152,154,156,158,195,695,42}) {
 var spell=new Spell{School=(MagicSchool)school,Category=(SpellCategory)category,IsPortalSpell=false};
 Console.WriteLine($"{school}\t{category}\t{new Creature().Route(spell,new object())}");
}
