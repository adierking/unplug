use crate::private::Sealed;
use crate::Object;
use phf::phf_map;
use std::fmt::{self, Debug, Formatter};
use unicase::UniCase;

/// Trait which allows accessing the animation list for an object.
pub trait ObjectAnimations: Sealed {
    /// Returns the list of known animations for this object.
    fn animations(self) -> &'static [Animation];
}

/// Metadata describing an animation.
struct Metadata {
    /// A unique name assigned by unplug-datagen.
    name: &'static str,
    object: Object,
    index: u16,
}

// Macro used in the generated animation list
macro_rules! declare_animations {
    {
        $(
            $obj:ident {
                $($index:literal => $id:ident { $name:tt }),*
                $(,)*
            }
        )*
    } => {
        /// An object animation.
        #[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub enum Animation {
            $(
                $($id),*
            ),*
        }

        const METADATA: &'static [Metadata] = &[
            $(
                $(Metadata {
                    name: $name,
                    object: Object::$obj,
                    index: $index,
                }),*
            ),*
        ];

        static LOOKUP: phf::Map<UniCase<&'static str>, Animation> = phf_map! {
            $(
                $(UniCase::ascii($name) => Animation::$id),*
            ),*
        };

        impl ObjectAnimations for Object {
            fn animations(self) -> &'static [Animation] {
                match self {
                    $(
                        Self::$obj => &[
                            $(Animation::$id),*
                        ],
                    )*
                    _ => &[],
                }
            }
        }
    };
}

impl Animation {
    /// Returns a string which uniquely identifies the animation.
    #[inline]
    pub fn name(self) -> &'static str {
        self.meta().name
    }

    /// Returns the object that the animation belongs to.
    #[inline]
    pub fn object(self) -> Object {
        self.meta().object
    }

    /// Returns the index of the animation within its object's animation list.
    #[inline]
    pub fn index(self) -> u16 {
        self.meta().index
    }

    /// Searches for the animation whose name matches `name` (case-insensitive).
    pub fn find(name: impl AsRef<str>) -> Option<Self> {
        LOOKUP.get(&UniCase::ascii(name.as_ref())).copied()
    }

    /// Gets an object's animation by index, returning `None` if the index has no known animation.
    pub fn get(object: Object, index: u16) -> Option<Self> {
        object.animations().get(index as usize).copied()
    }

    #[inline]
    fn meta(self) -> &'static Metadata {
        &METADATA[self as usize]
    }
}

impl Sealed for Animation {}

impl Debug for Animation {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "<{}>", self.name())
    }
}

// Generated using unplug-datagen
include!("gen/animations.inc.rs");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_animation() {
        let anim = Animation::TpBibiri;
        assert_eq!(anim.name(), "tp_bibiri");
        assert_eq!(anim.object(), Object::NpcTonpy);
        assert_eq!(anim.index(), 20);
        assert_eq!(format!("{:?}", anim), "<tp_bibiri>");
    }

    #[test]
    fn test_find_animation() {
        assert_eq!(Animation::find("tp_bibiri"), Some(Animation::TpBibiri));
        assert_eq!(Animation::find("Tp_BiBiRi"), Some(Animation::TpBibiri));
        assert_eq!(Animation::find("foo"), None);
    }

    #[test]
    fn test_object_animations() {
        assert_eq!(Object::NpcTonpy.animations()[20], Animation::TpBibiri);
        assert_eq!(Object::ItemCracker.animations(), &[]);
    }

    #[test]
    fn test_get_animation() {
        assert_eq!(Animation::get(Object::NpcTonpy, 20), Some(Animation::TpBibiri));
        assert_eq!(Animation::get(Object::NpcTonpy, 1000), None);
    }
}
