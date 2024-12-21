pub mod time {
    pub mod nanos {
        use serde::Serialize;
        use serde::Serializer;

        pub fn deserialize<'a, D: serde::Deserializer<'a>>(
            deserializer: D,
        ) -> Result<time::OffsetDateTime, D::Error> {
            let ts = <u64 as serde::Deserialize>::deserialize(deserializer)?;

            time::OffsetDateTime::from_unix_timestamp_nanos(ts as i128)
                .map_err(<D::Error as serde::de::Error>::custom)
        }

        pub fn serialize<S: Serializer>(
            datetime: &time::OffsetDateTime,
            serializer: S,
        ) -> Result<S::Ok, S::Error> {
            (datetime.unix_timestamp_nanos() as u64).serialize(serializer)
        }

        pub mod optional {
            use serde::Deserialize;
            use serde::Serialize;
            use serde::Serializer;
            use time::OffsetDateTime;

            pub fn deserialize<'a, D: serde::Deserializer<'a>>(
                deserializer: D,
            ) -> Result<Option<time::OffsetDateTime>, D::Error> {
                Option::deserialize(deserializer)?
                    .map(|u: u64| OffsetDateTime::from_unix_timestamp_nanos(u as i128))
                    .transpose()
                    .map_err(<D::Error as serde::de::Error>::custom)
            }

            pub fn serialize<S: Serializer>(
                option: &Option<time::OffsetDateTime>,
                serializer: S,
            ) -> Result<S::Ok, S::Error> {
                option
                    .map(|t| {
                        u64::try_from(t.unix_timestamp_nanos()).map_err(|_| {
                            serde::ser::Error::custom("OffsetDateTime does not fit in u64")
                        })
                    })
                    .transpose()?
                    .serialize(serializer)
            }
        }
    }
}
