using System;
using System.IO;
using System.Text;
using System.Linq;
using ACE.Server.Network;
using ACE.Server.Network.GameMessages;
static class CharacterLifecycleHarness {
    static void ClientString(BinaryWriter writer,string value) {
        writer.Write((ushort)value.Length);writer.Write(Encoding.UTF8.GetBytes(value));writer.Write(new byte[(4-(value.Length+2)%4)%4]);
    }
    public static object Run()=>new[]{GameMessageOpcode.CharacterEnterWorld,GameMessageOpcode.CharacterDelete,GameMessageOpcode.CharacterRestore}.Select(kind=>{
        using var ms=new MemoryStream();using var writer=new BinaryWriter(ms);writer.Write((uint)kind);
        switch(kind){case GameMessageOpcode.CharacterEnterWorld:writer.Write(0x50000001u);ClientString(writer,"Élodie😀");break;
            case GameMessageOpcode.CharacterDelete:ClientString(writer,"Élodie😀");writer.Write(3u);break;
            case GameMessageOpcode.CharacterRestore:writer.Write(0x50000001u);break;}
        writer.Write(new byte[]{0xa5,0x5a});var bytes=ms.ToArray();var message=new ClientMessage(bytes);
        var captured=kind switch {GameMessageOpcode.CharacterEnterWorld=>ExtractedLifecycleReaders.CharacterEnterWorld(message),GameMessageOpcode.CharacterDelete=>ExtractedLifecycleReaders.CharacterDelete(message),_=>ExtractedLifecycleReaders.CharacterRestore(message)};
        return new{bytes=Convert.ToHexString(bytes),captured,trailing_bytes=message.Data.Length-message.Data.Position};
    }).ToArray();
}
