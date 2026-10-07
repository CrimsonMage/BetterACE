"""Verbatim input-reading prefixes; omitted code is gameplay/routing policy.
Each extraction is identified in fixture provenance and compiled, never rewritten.
"""
NETWORK = 'Source/ACE.Server/Network/'
PREFIXES = {
    'Talk': ('var message = clientMessage.Payload.ReadString16L();', 'if (message.StartsWith("@"))', 'new { text = message }'),
    'Tell': ('var message = clientMessage.Payload.ReadString16L();', 'if (session.Player.IsGagged)', 'new { text = message, target_name = target }'),
    'TalkDirect': ('var message = clientMessage.Payload.ReadString16L();', 'var creature =', 'new { text = message, target_id = targetGuid }'),
    'ChatChannel': ('var groupChatType =', 'switch (groupChatType)', 'new { channel = (uint)groupChatType, text = message }'),
    'AddChannel': ('var chatChannelID =', 'if (session.AccessLevel', 'new { channel = (uint)chatChannelID }'),
    'RemoveChannel': ('var chatChannelID =', 'if (session.AccessLevel', 'new { channel = (uint)chatChannelID }'),
}
SOURCES = [NETWORK + 'GameAction/Actions/GameAction' + name + '.cs' for name in PREFIXES]
TURBINE = NETWORK + 'Handlers/TurbineChatHandler.cs'
SQUELCH = NETWORK + 'Structure/SquelchDB.cs'
SOURCES += [TURBINE, SQUELCH]

def generate(sources, extract_method):
    methods = []
    for name, (start, end, result) in PREFIXES.items():
        source = sources[NETWORK + 'GameAction/Actions/GameAction' + name + '.cs'].decode('utf-8-sig')
        body = source[source.index(start):source.index(end, source.index(start))]
        variable = 'message' if name in ['AddChannel','RemoveChannel'] else 'clientMessage'
        methods.append('public static object ' + name + '(ClientMessage ' + variable + ') { ' + body + ' return ' + result + '; }')
    source = sources[TURBINE].decode('utf-8-sig')
    start = source.index('clientMessage.Payload.ReadUInt32(); // Bytes to follow')
    header = source[start:source.index('if (session.Player.IsGagged)', start)]
    start = source.index('var contextId =')
    payload = source[start:source.index('var adjustedChannelID =', start)]
    methods.append('public static object Turbine(ClientMessage clientMessage) { ' + header + payload + ' return new { blob_type = (uint)chatBlobType, dispatch = (uint)chatBlobDispatchType, context_id = contextId, channel = channelID, text = message, sender_id = senderID, chat_type = (uint)chatType }; }')
    prefix = 'using System; using System.Text; using System.IO; using ACE.Common.Extensions; using ACE.Entity.Enum; using ACE.Server.Network; public static class ExtractedSocialReaders { '
    readers = prefix + '\n'.join(methods) + ' }'
    serializer = 'using System.IO; using System.Collections.Generic; namespace ACE.Server.Network.Structure { ' + extract_method(sources[SQUELCH].decode('utf-8-sig'), 'public static class SquelchDBExtensions') + ' }'
    return readers, serializer
