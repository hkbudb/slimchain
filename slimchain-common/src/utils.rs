pub use derive_more;
pub use hex;

#[macro_export]
macro_rules! create_id_type {
    ($name: ident, $inner_type: ty, $atomic_type: ty) => {
        #[derive(
            Debug,
            Default,
            Copy,
            Clone,
            Eq,
            PartialEq,
            Ord,
            PartialOrd,
            Hash,
            serde::Serialize,
            serde::Deserialize,
            $crate::utils::derive_more::Deref,
            $crate::utils::derive_more::DerefMut,
            $crate::utils::derive_more::Display,
            $crate::utils::derive_more::From,
            $crate::utils::derive_more::Into,
        )]
        pub struct $name(pub $inner_type);

        impl $name {
            pub fn next_id() -> Self {
                static ID_CNT: $atomic_type = <$atomic_type>::new(0);
                Self(ID_CNT.fetch_add(1, core::sync::atomic::Ordering::SeqCst))
            }
        }
    };
}

#[macro_export]
macro_rules! create_id_type_u32 {
    ($name: ident) => {
        $crate::create_id_type!($name, u32, core::sync::atomic::AtomicU32);
    };
}

#[macro_export]
macro_rules! create_id_type_u64 {
    ($name: ident) => {
        $crate::create_id_type!($name, u64, core::sync::atomic::AtomicU64);
    };
}

#[macro_export]
macro_rules! create_id_type_usize {
    ($name: ident) => {
        $crate::create_id_type!($name, usize, core::sync::atomic::AtomicUsize);
    };
}

#[macro_export]
macro_rules! create_address {
    ($x:expr) => {
        $crate::basic::Address::from($crate::basic::H160::from_slice(
            &$crate::utils::hex::decode($x).unwrap()[..],
        ))
    };
}

#[macro_export]
macro_rules! create_state_key {
    ($x:expr) => {
        $crate::basic::StateKey::from($crate::basic::H256::from_slice(
            &$crate::utils::hex::decode($x).unwrap()[..],
        ))
    };
}

#[macro_export]
macro_rules! create_acc_write_set {
    ($($key:ident: $value:tt,)+) => { $crate::create_acc_write_set!($($key: $value),+) };
    ($($key:ident: $value:tt),*) => {
        {
            let mut writes = $crate::rw_set::AccountWriteData::default();
            $(
                $crate::create_acc_write_set!(@data writes @parse $key $value);
            )*
            writes
        }
    };
    (@data $writes:ident @parse nonce $x:expr) => {
        $writes.nonce = Some($crate::basic::Nonce::from($x));
    };
    (@data $writes:ident @parse reset_values $x:expr) => {
        $writes.reset_values = $x;
    };
    (@data $writes:ident @parse code $x:expr) => {
        $writes.code = Some($crate::basic::Code::from($x.to_vec()));
    };
    (@data $writes:ident @parse values { $($key:expr => $value:expr,)+ }) => {
        $crate::create_acc_write_set!(@data $writes @parse values { $($key => $value),+ });
    };
    (@data $writes:ident @parse values { $($key:expr => $value:expr),* }) => {
        {
            $(
                $writes.values.insert(
                    $crate::create_state_key!($key),
                    ($value as u64).into()
                );
            )*
        }
    };
}

#[macro_export]
macro_rules! create_tx_write_set {
    ($($addr:expr => $value:tt,)+) => { $crate::create_tx_write_set!($($addr => $value),+) };
    ($($addr:expr => $value:tt),*) => {
        {
            let mut out = $crate::rw_set::TxWriteData::default();
            $(
                let addr = $crate::create_address!($addr);
                let writes = $crate::create_acc_write_set! $value;
                out.insert(addr, writes);
            )*
            out
        }
    };
}

#[macro_export]
macro_rules! create_acc_read_set {
    ($($key:ident: $value:tt,)+) => { $crate::create_acc_read_set!($($key: $value),+) };
    ($($key:ident: $value:tt),*) => {
        {
            let mut reads = $crate::rw_set::AccountReadSet::default();
            $(
                $crate::create_acc_read_set!(@data reads @parse $key $value);
            )*
            reads
        }
    };
    (@data $reads:ident @parse nonce $x:expr) => {
        $reads.set_nonce($x);
    };
    (@data $reads:ident @parse code $x:expr) => {
        $reads.set_code($x);
    };
    (@data $reads:ident @parse values [ $($key:expr,)+ ]) => {
        $crate::create_acc_read_set!(@data $reads @parse values [ $($key),+ ]);
    };
    (@data $reads:ident @parse values [ $($key:expr),* ]) => {
        {
            $(
                $reads.values.insert($crate::create_state_key!($key));
            )*
        }
    };
}

#[macro_export]
macro_rules! create_tx_read_set {
    ($($addr:expr => $value:tt,)+) => { $crate::create_tx_read_set!($($addr => $value),+) };
    ($($addr:expr => $value:tt),*) => {
        {
            let mut out = $crate::rw_set::TxReadSet::default();
            $(
                let addr = $crate::create_address!($addr);
                let reads = $crate::create_acc_read_set! $value;
                out.insert(addr, reads);
            )*
            out
        }
    };
}

#[macro_export]
macro_rules! create_acc_read_data {
    ($($key:ident: $value:tt,)+) => { $crate::create_acc_read_data!($($key: $value),+) };
    ($($key:ident: $value:tt),*) => {
        {
            let mut reads = $crate::rw_set::AccountReadData::default();
            $(
                $crate::create_acc_read_data!(@data reads @parse $key $value);
            )*
            reads
        }
    };
    (@data $reads:ident @parse nonce $x:expr) => {
        $reads.nonce = Some($crate::basic::Nonce::from($x));
    };
    (@data $reads:ident @parse code $x:expr) => {
        $reads.code = Some($crate::basic::Code::from($x.to_vec()));
    };
    (@data $reads:ident @parse values { $($key:expr => $value:expr,)+ }) => {
        $crate::create_acc_read_data!(@data $reads @parse values { $($key => $value),+ });
    };
    (@data $reads:ident @parse values { $($key:expr => $value:expr),* }) => {
        {
            $(
                $reads.values.insert(
                    $crate::create_state_key!($key),
                    ($value as u64).into()
                );
            )*
        }
    };
}

#[macro_export]
macro_rules! create_tx_read_data {
    ($($addr:expr => $value:tt,)+) => { $crate::create_tx_read_data!($($addr => $value),+) };
    ($($addr:expr => $value:tt),*) => {
        {
            let mut out = $crate::rw_set::TxReadData::default();
            $(
                let addr = $crate::create_address!($addr);
                let reads = $crate::create_acc_read_data! $value;
                out.insert(addr, reads);
            )*
            out
        }
    };
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_create_tx_write_set() {
        let _ = create_tx_write_set! {
            "0000000000000000000000000000000000000000" => {
                nonce: 1,
            },
            "0000000000000000000000000000000000000001" => {
                reset_values: true,
                code: b"code",
                values: {
                    "0000000000000000000000000000000000000000000000000000000000000000" => 1,
                    "0000000000000000000000000000000000000000000000000000000000000001" => 2,
                }
            }
        };
    }

    #[test]
    fn test_create_tx_read_data_and_read_set() {
        let read_data = create_tx_read_data! {
            "0000000000000000000000000000000000000000" => {
                nonce: 1,
            },
            "0000000000000000000000000000000000000001" => {
                code: b"code",
                values: {
                    "0000000000000000000000000000000000000000000000000000000000000000" => 1,
                    "0000000000000000000000000000000000000000000000000000000000000001" => 2,
                }
            }
        };
        let read_set = create_tx_read_set! {
            "0000000000000000000000000000000000000000" => {
                nonce: true,
            },
            "0000000000000000000000000000000000000001" => {
                code: true,
                values: [
                    "0000000000000000000000000000000000000000000000000000000000000000",
                    "0000000000000000000000000000000000000000000000000000000000000001",
                ]
            }
        };
        assert_eq!(read_data.to_set(), read_set);
    }
}
