//! Pinned ACE Entity/Mutations/Recipes tables; AGPL-3.0-only.
use crate::scripts::{Argument, Effect, Op};
use crate::{PropertyKey, PropertyKind as K, PropertyValue as V};

pub(crate) fn effects(id: u32) -> Option<&'static [Effect]> {
    Some(match id {
        0x3800000F => &[Effect {
            key: PropertyKey {
                kind: K::DataId,
                id: 50,
            },
            op: Op::Assign,
            arg: Argument::Quality(PropertyKey {
                kind: K::DataId,
                id: 51,
            }),
            second: None,
        }],
        0x38000011 => &[
            Effect {
                key: PropertyKey {
                    kind: K::Int,
                    id: 28,
                },
                op: Op::Add,
                arg: Argument::Literal(V::Int(20)),
                second: None,
            },
            Effect {
                key: PropertyKey {
                    kind: K::Int,
                    id: 171,
                },
                op: Op::AtLeastAdd,
                arg: Argument::Literal(V::Int(1)),
                second: Some(Argument::Literal(V::Int(1))),
            },
        ],
        0x38000012 => &[
            Effect {
                key: PropertyKey {
                    kind: K::Float,
                    id: 18,
                },
                op: Op::Add,
                arg: Argument::Literal(V::Float(0.4)),
                second: None,
            },
            Effect {
                key: PropertyKey {
                    kind: K::Int,
                    id: 171,
                },
                op: Op::AtLeastAdd,
                arg: Argument::Literal(V::Int(1)),
                second: Some(Argument::Literal(V::Int(1))),
            },
        ],
        0x38000013 => &[
            Effect {
                key: PropertyKey {
                    kind: K::Float,
                    id: 15,
                },
                op: Op::Add,
                arg: Argument::Literal(V::Float(0.2)),
                second: None,
            },
            Effect {
                key: PropertyKey {
                    kind: K::Int,
                    id: 171,
                },
                op: Op::AtLeastAdd,
                arg: Argument::Literal(V::Int(1)),
                second: Some(Argument::Literal(V::Int(1))),
            },
        ],
        0x38000014 => &[
            Effect {
                key: PropertyKey {
                    kind: K::Float,
                    id: 16,
                },
                op: Op::Add,
                arg: Argument::Literal(V::Float(0.4)),
                second: None,
            },
            Effect {
                key: PropertyKey {
                    kind: K::Int,
                    id: 171,
                },
                op: Op::AtLeastAdd,
                arg: Argument::Literal(V::Int(1)),
                second: Some(Argument::Literal(V::Int(1))),
            },
        ],
        0x38000015 => &[
            Effect {
                key: PropertyKey {
                    kind: K::Float,
                    id: 19,
                },
                op: Op::Add,
                arg: Argument::Literal(V::Float(0.4)),
                second: None,
            },
            Effect {
                key: PropertyKey {
                    kind: K::Int,
                    id: 171,
                },
                op: Op::AtLeastAdd,
                arg: Argument::Literal(V::Int(1)),
                second: Some(Argument::Literal(V::Int(1))),
            },
        ],
        0x38000016 => &[
            Effect {
                key: PropertyKey {
                    kind: K::Float,
                    id: 17,
                },
                op: Op::Add,
                arg: Argument::Literal(V::Float(0.4)),
                second: None,
            },
            Effect {
                key: PropertyKey {
                    kind: K::Int,
                    id: 171,
                },
                op: Op::AtLeastAdd,
                arg: Argument::Literal(V::Int(1)),
                second: Some(Argument::Literal(V::Int(1))),
            },
        ],
        0x38000017 => &[
            Effect {
                key: PropertyKey {
                    kind: K::Float,
                    id: 14,
                },
                op: Op::Add,
                arg: Argument::Literal(V::Float(0.2)),
                second: None,
            },
            Effect {
                key: PropertyKey {
                    kind: K::Int,
                    id: 171,
                },
                op: Op::AtLeastAdd,
                arg: Argument::Literal(V::Int(1)),
                second: Some(Argument::Literal(V::Int(1))),
            },
        ],
        0x38000018 => &[
            Effect {
                key: PropertyKey {
                    kind: K::Float,
                    id: 13,
                },
                op: Op::Add,
                arg: Argument::Literal(V::Float(0.2)),
                second: None,
            },
            Effect {
                key: PropertyKey {
                    kind: K::Int,
                    id: 171,
                },
                op: Op::AtLeastAdd,
                arg: Argument::Literal(V::Int(1)),
                second: Some(Argument::Literal(V::Int(1))),
            },
        ],
        0x38000019 => &[
            Effect {
                key: PropertyKey {
                    kind: K::Int,
                    id: 5,
                },
                op: Op::Multiply,
                arg: Argument::Literal(V::Float(0.75)),
                second: None,
            },
            Effect {
                key: PropertyKey {
                    kind: K::Int,
                    id: 171,
                },
                op: Op::AtLeastAdd,
                arg: Argument::Literal(V::Int(1)),
                second: Some(Argument::Literal(V::Int(1))),
            },
        ],
        0x3800001A => &[
            Effect {
                key: PropertyKey {
                    kind: K::Int,
                    id: 44,
                },
                op: Op::Add,
                arg: Argument::Literal(V::Int(1)),
                second: None,
            },
            Effect {
                key: PropertyKey {
                    kind: K::Int,
                    id: 171,
                },
                op: Op::AtLeastAdd,
                arg: Argument::Literal(V::Int(1)),
                second: Some(Argument::Literal(V::Int(1))),
            },
        ],
        0x3800001B => &[
            Effect {
                key: PropertyKey {
                    kind: K::Float,
                    id: 63,
                },
                op: Op::Add,
                arg: Argument::Literal(V::Float(0.04)),
                second: None,
            },
            Effect {
                key: PropertyKey {
                    kind: K::Int,
                    id: 171,
                },
                op: Op::AtLeastAdd,
                arg: Argument::Literal(V::Int(1)),
                second: Some(Argument::Literal(V::Int(1))),
            },
        ],
        0x3800001C => &[
            Effect {
                key: PropertyKey {
                    kind: K::Float,
                    id: 22,
                },
                op: Op::Multiply,
                arg: Argument::Literal(V::Float(0.8)),
                second: None,
            },
            Effect {
                key: PropertyKey {
                    kind: K::Int,
                    id: 171,
                },
                op: Op::AtLeastAdd,
                arg: Argument::Literal(V::Int(1)),
                second: Some(Argument::Literal(V::Int(1))),
            },
        ],
        0x3800001D => &[
            Effect {
                key: PropertyKey {
                    kind: K::Int,
                    id: 49,
                },
                op: Op::Subtract,
                arg: Argument::Literal(V::Int(50)),
                second: None,
            },
            Effect {
                key: PropertyKey {
                    kind: K::Int,
                    id: 49,
                },
                op: Op::AtLeastAdd,
                arg: Argument::Literal(V::Int(0)),
                second: Some(Argument::Literal(V::Int(0))),
            },
            Effect {
                key: PropertyKey {
                    kind: K::Int,
                    id: 171,
                },
                op: Op::AtLeastAdd,
                arg: Argument::Literal(V::Int(1)),
                second: Some(Argument::Literal(V::Int(1))),
            },
        ],
        0x3800001E => &[
            Effect {
                key: PropertyKey {
                    kind: K::Int,
                    id: 19,
                },
                op: Op::Multiply,
                arg: Argument::Literal(V::Float(0.75)),
                second: None,
            },
            Effect {
                key: PropertyKey {
                    kind: K::Int,
                    id: 171,
                },
                op: Op::AtLeastAdd,
                arg: Argument::Literal(V::Int(1)),
                second: Some(Argument::Literal(V::Int(1))),
            },
        ],
        0x3800001F => &[
            Effect {
                key: PropertyKey {
                    kind: K::Int,
                    id: 19,
                },
                op: Op::Multiply,
                arg: Argument::Literal(V::Float(1.25)),
                second: None,
            },
            Effect {
                key: PropertyKey {
                    kind: K::Int,
                    id: 171,
                },
                op: Op::AtLeastAdd,
                arg: Argument::Literal(V::Int(1)),
                second: Some(Argument::Literal(V::Int(1))),
            },
        ],
        0x38000020 => &[
            Effect {
                key: PropertyKey {
                    kind: K::Float,
                    id: 29,
                },
                op: Op::Add,
                arg: Argument::Literal(V::Float(0.01)),
                second: None,
            },
            Effect {
                key: PropertyKey {
                    kind: K::Int,
                    id: 171,
                },
                op: Op::AtLeastAdd,
                arg: Argument::Literal(V::Int(1)),
                second: Some(Argument::Literal(V::Int(1))),
            },
        ],
        0x38000021 => &[
            Effect {
                key: PropertyKey {
                    kind: K::Float,
                    id: 62,
                },
                op: Op::Add,
                arg: Argument::Literal(V::Float(0.01)),
                second: None,
            },
            Effect {
                key: PropertyKey {
                    kind: K::Int,
                    id: 171,
                },
                op: Op::AtLeastAdd,
                arg: Argument::Literal(V::Int(1)),
                second: Some(Argument::Literal(V::Int(1))),
            },
        ],
        0x38000023 => &[
            Effect {
                key: PropertyKey {
                    kind: K::Int,
                    id: 179,
                },
                op: Op::Assign,
                arg: Argument::Literal(V::Int(1)),
                second: None,
            },
            Effect {
                key: PropertyKey {
                    kind: K::Int,
                    id: 171,
                },
                op: Op::AtLeastAdd,
                arg: Argument::Literal(V::Int(1)),
                second: Some(Argument::Literal(V::Int(1))),
            },
        ],
        0x38000024 => &[
            Effect {
                key: PropertyKey {
                    kind: K::Int,
                    id: 179,
                },
                op: Op::Assign,
                arg: Argument::Literal(V::Int(2)),
                second: None,
            },
            Effect {
                key: PropertyKey {
                    kind: K::Int,
                    id: 171,
                },
                op: Op::AtLeastAdd,
                arg: Argument::Literal(V::Int(1)),
                second: Some(Argument::Literal(V::Int(1))),
            },
        ],
        0x38000025 => &[
            Effect {
                key: PropertyKey {
                    kind: K::Int,
                    id: 179,
                },
                op: Op::Assign,
                arg: Argument::Literal(V::Int(4)),
                second: None,
            },
            Effect {
                key: PropertyKey {
                    kind: K::Int,
                    id: 171,
                },
                op: Op::AtLeastAdd,
                arg: Argument::Literal(V::Int(1)),
                second: Some(Argument::Literal(V::Int(1))),
            },
        ],
        0x3800002E => &[
            Effect {
                key: PropertyKey {
                    kind: K::Float,
                    id: 144,
                },
                op: Op::AtLeastAdd,
                arg: Argument::Literal(V::Float(0.01)),
                second: Some(Argument::Literal(V::Float(0.01))),
            },
            Effect {
                key: PropertyKey {
                    kind: K::Int,
                    id: 171,
                },
                op: Op::AtLeastAdd,
                arg: Argument::Literal(V::Int(1)),
                second: Some(Argument::Literal(V::Int(1))),
            },
        ],
        0x3800002F => &[
            Effect {
                key: PropertyKey {
                    kind: K::Int,
                    id: 108,
                },
                op: Op::Add,
                arg: Argument::Literal(V::Int(500)),
                second: None,
            },
            Effect {
                key: PropertyKey {
                    kind: K::Int,
                    id: 171,
                },
                op: Op::AtLeastAdd,
                arg: Argument::Literal(V::Int(1)),
                second: Some(Argument::Literal(V::Int(1))),
            },
        ],
        0x38000034 => &[
            Effect {
                key: PropertyKey {
                    kind: K::DataId,
                    id: 37,
                },
                op: Op::Assign,
                arg: Argument::Literal(V::DataId(7)),
                second: None,
            },
            Effect {
                key: PropertyKey {
                    kind: K::Int,
                    id: 115,
                },
                op: Op::Multiply,
                arg: Argument::Literal(V::Float(0.7)),
                second: None,
            },
            Effect {
                key: PropertyKey {
                    kind: K::Int,
                    id: 171,
                },
                op: Op::AtLeastAdd,
                arg: Argument::Literal(V::Int(1)),
                second: Some(Argument::Literal(V::Int(1))),
            },
        ],
        0x38000035 => &[
            Effect {
                key: PropertyKey {
                    kind: K::DataId,
                    id: 37,
                },
                op: Op::Assign,
                arg: Argument::Literal(V::DataId(6)),
                second: None,
            },
            Effect {
                key: PropertyKey {
                    kind: K::Int,
                    id: 115,
                },
                op: Op::Divide,
                arg: Argument::Literal(V::Float(0.7)),
                second: None,
            },
            Effect {
                key: PropertyKey {
                    kind: K::Int,
                    id: 171,
                },
                op: Op::AtLeastAdd,
                arg: Argument::Literal(V::Int(1)),
                second: Some(Argument::Literal(V::Int(1))),
            },
        ],
        0x38000036 => &[
            Effect {
                key: PropertyKey {
                    kind: K::Int,
                    id: 110,
                },
                op: Op::Assign,
                arg: Argument::Literal(V::Int(0)),
                second: None,
            },
            Effect {
                key: PropertyKey {
                    kind: K::Int,
                    id: 109,
                },
                op: Op::Assign,
                arg: Argument::Quality(PropertyKey {
                    kind: K::Int,
                    id: 106,
                }),
                second: None,
            },
            Effect {
                key: PropertyKey {
                    kind: K::Int,
                    id: 171,
                },
                op: Op::AtLeastAdd,
                arg: Argument::Literal(V::Int(1)),
                second: Some(Argument::Literal(V::Int(1))),
            },
        ],
        0x38000037 => &[
            Effect {
                key: PropertyKey {
                    kind: K::Int,
                    id: 179,
                },
                op: Op::Assign,
                arg: Argument::Literal(V::Int(4096)),
                second: None,
            },
            Effect {
                key: PropertyKey {
                    kind: K::Int,
                    id: 171,
                },
                op: Op::AtLeastAdd,
                arg: Argument::Literal(V::Int(1)),
                second: Some(Argument::Literal(V::Int(1))),
            },
        ],
        0x38000038 => &[
            Effect {
                key: PropertyKey {
                    kind: K::Int,
                    id: 179,
                },
                op: Op::Assign,
                arg: Argument::Literal(V::Int(1024)),
                second: None,
            },
            Effect {
                key: PropertyKey {
                    kind: K::Int,
                    id: 171,
                },
                op: Op::AtLeastAdd,
                arg: Argument::Literal(V::Int(1)),
                second: Some(Argument::Literal(V::Int(1))),
            },
        ],
        0x38000039 => &[
            Effect {
                key: PropertyKey {
                    kind: K::Int,
                    id: 179,
                },
                op: Op::Assign,
                arg: Argument::Literal(V::Int(2048)),
                second: None,
            },
            Effect {
                key: PropertyKey {
                    kind: K::Int,
                    id: 171,
                },
                op: Op::AtLeastAdd,
                arg: Argument::Literal(V::Int(1)),
                second: Some(Argument::Literal(V::Int(1))),
            },
        ],
        0x3800003A => &[
            Effect {
                key: PropertyKey {
                    kind: K::Int,
                    id: 179,
                },
                op: Op::Assign,
                arg: Argument::Literal(V::Int(64)),
                second: None,
            },
            Effect {
                key: PropertyKey {
                    kind: K::Int,
                    id: 171,
                },
                op: Op::AtLeastAdd,
                arg: Argument::Literal(V::Int(1)),
                second: Some(Argument::Literal(V::Int(1))),
            },
        ],
        0x3800003B => &[
            Effect {
                key: PropertyKey {
                    kind: K::Int,
                    id: 179,
                },
                op: Op::Assign,
                arg: Argument::Literal(V::Int(32)),
                second: None,
            },
            Effect {
                key: PropertyKey {
                    kind: K::Int,
                    id: 171,
                },
                op: Op::AtLeastAdd,
                arg: Argument::Literal(V::Int(1)),
                second: Some(Argument::Literal(V::Int(1))),
            },
        ],
        0x3800003C => &[
            Effect {
                key: PropertyKey {
                    kind: K::Int,
                    id: 179,
                },
                op: Op::Assign,
                arg: Argument::Literal(V::Int(128)),
                second: None,
            },
            Effect {
                key: PropertyKey {
                    kind: K::Int,
                    id: 171,
                },
                op: Op::AtLeastAdd,
                arg: Argument::Literal(V::Int(1)),
                second: Some(Argument::Literal(V::Int(1))),
            },
        ],
        0x3800003D => &[
            Effect {
                key: PropertyKey {
                    kind: K::Int,
                    id: 179,
                },
                op: Op::Assign,
                arg: Argument::Literal(V::Int(256)),
                second: None,
            },
            Effect {
                key: PropertyKey {
                    kind: K::Int,
                    id: 171,
                },
                op: Op::AtLeastAdd,
                arg: Argument::Literal(V::Int(1)),
                second: Some(Argument::Literal(V::Int(1))),
            },
        ],
        0x3800003E => &[
            Effect {
                key: PropertyKey {
                    kind: K::Int,
                    id: 179,
                },
                op: Op::Assign,
                arg: Argument::Literal(V::Int(512)),
                second: None,
            },
            Effect {
                key: PropertyKey {
                    kind: K::Int,
                    id: 171,
                },
                op: Op::AtLeastAdd,
                arg: Argument::Literal(V::Int(1)),
                second: Some(Argument::Literal(V::Int(1))),
            },
        ],
        0x3800003F => &[
            Effect {
                key: PropertyKey {
                    kind: K::Int,
                    id: 179,
                },
                op: Op::Assign,
                arg: Argument::Literal(V::Int(16)),
                second: None,
            },
            Effect {
                key: PropertyKey {
                    kind: K::Int,
                    id: 171,
                },
                op: Op::AtLeastAdd,
                arg: Argument::Literal(V::Int(1)),
                second: Some(Argument::Literal(V::Int(1))),
            },
        ],
        0x38000040 => &[
            Effect {
                key: PropertyKey {
                    kind: K::Int,
                    id: 179,
                },
                op: Op::Assign,
                arg: Argument::Literal(V::Int(8)),
                second: None,
            },
            Effect {
                key: PropertyKey {
                    kind: K::Int,
                    id: 171,
                },
                op: Op::AtLeastAdd,
                arg: Argument::Literal(V::Int(1)),
                second: Some(Argument::Literal(V::Int(1))),
            },
        ],
        0x38000041 => &[
            Effect {
                key: PropertyKey {
                    kind: K::Int,
                    id: 179,
                },
                op: Op::Assign,
                arg: Argument::Literal(V::Int(8192)),
                second: None,
            },
            Effect {
                key: PropertyKey {
                    kind: K::Int,
                    id: 171,
                },
                op: Op::AtLeastAdd,
                arg: Argument::Literal(V::Int(1)),
                second: Some(Argument::Literal(V::Int(1))),
            },
        ],
        0x38000042 => &[Effect {
            key: PropertyKey {
                kind: K::Int,
                id: 171,
            },
            op: Op::AtLeastAdd,
            arg: Argument::Literal(V::Int(1)),
            second: Some(Argument::Literal(V::Int(1))),
        }],
        0x38000043 => &[Effect {
            key: PropertyKey {
                kind: K::Bool,
                id: 91,
            },
            op: Op::Assign,
            arg: Argument::Literal(V::Bool(true)),
            second: None,
        }],
        0x38000046 => &[
            Effect {
                key: PropertyKey {
                    kind: K::Int,
                    id: 179,
                },
                op: Op::AtLeastAdd,
                arg: Argument::Literal(V::Int(0)),
                second: Some(Argument::Literal(V::Int(0))),
            },
            Effect {
                key: PropertyKey {
                    kind: K::Int,
                    id: 179,
                },
                op: Op::Add,
                arg: Argument::Literal(V::Int(536870912)),
                second: None,
            },
        ],
        0x3800004B => &[
            Effect {
                key: PropertyKey {
                    kind: K::Float,
                    id: 152,
                },
                op: Op::AtLeastAdd,
                arg: Argument::Literal(V::Float(0.01)),
                second: Some(Argument::Literal(V::Float(0.01))),
            },
            Effect {
                key: PropertyKey {
                    kind: K::Int,
                    id: 171,
                },
                op: Op::AtLeastAdd,
                arg: Argument::Literal(V::Int(1)),
                second: Some(Argument::Literal(V::Int(1))),
            },
        ],
        0x3800004E => &[Effect {
            key: PropertyKey {
                kind: K::Bool,
                id: 91,
            },
            op: Op::Assign,
            arg: Argument::Literal(V::Bool(false)),
            second: None,
        }],
        0x39000000 => &[
            Effect {
                key: PropertyKey {
                    kind: K::Int,
                    id: 319,
                },
                op: Op::AtLeastAdd,
                arg: Argument::Literal(V::Int(1)),
                second: Some(Argument::Literal(V::Int(1))),
            },
            Effect {
                key: PropertyKey {
                    kind: K::Int64,
                    id: 5,
                },
                op: Op::Assign,
                arg: Argument::Literal(V::Int64(2000000000)),
                second: None,
            },
            Effect {
                key: PropertyKey {
                    kind: K::Int64,
                    id: 4,
                },
                op: Op::AtLeastAdd,
                arg: Argument::Literal(V::Int64(0)),
                second: Some(Argument::Literal(V::Int64(0))),
            },
        ],
        0x39000001 => &[Effect {
            key: PropertyKey {
                kind: K::Float,
                id: 22,
            },
            op: Op::Multiply,
            arg: Argument::Literal(V::Float(0.8)),
            second: None,
        }],
        _ => return None,
    })
}
