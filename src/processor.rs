use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Processor {
    Jito,
    Nextblock,
    Sender,
    Zeroslot,
    Bloxroute,
    Astralane,
    Blockrazor,
}

impl Processor {
    pub fn all() -> &'static [Processor] {
        &[
            Processor::Jito,
            Processor::Nextblock,
            Processor::Sender,
            Processor::Zeroslot,
            Processor::Bloxroute,
            Processor::Astralane,
            Processor::Blockrazor,
        ]
    }

    pub fn accounts(&self) -> &'static [&'static str] {
        match self {
            Processor::Jito => &[
                "96gYZGLnJYVFmbjzopPSU6QiEV5fGqZNyN9nmNhvrZU5",
                "HFqU5x63VTqvQss8hp11i4wVV8bD44PvwucfZ2bU7gRe",
                "Cw8CFyM9FkoMi7K7Crf6HNQqf4uEMzpKw6QNghXLvLkY",
                "ADaUMid9yfUytqMBgopwjb2DTLSokTSzL1zt6iGPaS49",
                "DfXygSm4jCyNCybVYYK6DwvWqjKee8pbDmJGcLWNDXjh",
                "ADuUkR4vqLUMWXxW9gh6D6L8pMSawimctcNZ5pGwDcEt",
                "DttWaMuVvTiduZRnguLF7jNxTgiMBZ1hyAumKUiL2KRL",
                "3AVi9Tg9Uo68tJfuvoKvqKNWKkC5wPdSSdeBnizKZ6jT",
            ],
            Processor::Nextblock => &[
                "NextbLoCkVtMGcV47JzewQdvBpLqT9TxQFozQkN98pE",
                "NexTbLoCkWykbLuB1NkjXgFWkX9oAtcoagQegygXXA2",
                "NeXTBLoCKs9F1y5PJS9CKrFNNLU1keHW71rfh7KgA1X",
                "NexTBLockJYZ7QD7p2byrUa6df8ndV2WSd8GkbWqfbb",
                "neXtBLock1LeC67jYd1QdAa32kbVeubsfPNTJC1V5At",
                "nEXTBLockYgngeRmRrjDV31mGSekVPqZoMGhQEZtPVG",
                "NEXTbLoCkB51HpLBLojQfpyVAMorm3zzKg7w9NFdqid",
                "nextBLoCkPMgmG8ZgJtABeScP35qLa2AMCNKntAP7Xc",
            ],
            Processor::Sender => &[
                "4ACfpUFoaSD9bfPdeu6DBt89gB6ENTeHBXCAi87NhDEE",
                "D2L6yPZ2FmmmTKPgzaMKdhu6EWZcTpLy1Vhx8uvZe7NZ",
                "9bnz4RShgq1hAnLnZbP8kbgBg1kEmcJBYQq3gQbmnSta",
                "5VY91ws6B2hMmBFRsXkoAAdsPHBJwRfBht4DXox3xkwn",
                "2nyhqdwKcJZR2vcqCyrYsaPVdAnFoJjiksCXJ7hfEYgD",
                "2q5pghRs6arqVjRvT5gfgWfWcHWmw1ZuCzphgd5KfWGJ",
                "wyvPkWjVZz1M8fHQnMMCDTQDbkManefNNhweYk5WkcF",
                "3KCKozbAaF75qEU33jtzozcJ29yJuaLJTy2jFdzUY8bT",
                "4vieeGHPYPG2MmyPRcYjdiDmmhN3ww7hsFNap8pVN3Ey",
                "4TQLFNWK8AovT1gFvda5jfw2oJeRMKEmw7aH6MGBJ3or",
            ],
            Processor::Zeroslot => &[
                "Eb2KpSC8uMt9GmzyAEm5Eb1AAAgTjRaXWFjKyFXHZxF3",
                "FCjUJZ1qozm1e8romw216qyfQMaaWKxWsuySnumVCCNe",
                "ENxTEjSQ1YabmUpXAdCgevnHQ9MHdLv8tzFiuiYJqa13",
                "6rYLG55Q9RpsPGvqdPNJs4z5WTxJVatMB8zV3WJhs5EK",
                "Cix2bHfqPcKcM233mzxbLk14kSggUUiz2A87fJtGivXr",
            ],
            Processor::Bloxroute => &[
                "HWEoBxYs7ssKuudEjzjmpfJVX7Dvi7wescFsVx2L5yoY",
                "95cfoy472fcQHaw4tPGBTKpn6ZQnfEPfBgDQx6gcRmRg",
            ],
            Processor::Astralane => &[
                "astrazznxsGUhWShqgNtAdfrzP2G83DzcWVJDxwV9bF",
                "astra4uejePWneqNaJKuFFA8oonqCE1sqF6b45kDMZm",
                "astra9xWY93QyfG6yM8zwsKsRodscjQ2uU2HKNL5prk",
                "astraRVUuTHjpwEVvNBeQEgwYx9w9CFyfxjYoobCZhL",
                "astraEJ2fEj8Xmy6KLG7B3VfbKfsHXhHrNdCQx7iGJK",
                "astraZW5GLFefxNPAatceHhYjfA1ciq9gvfEg2S47xk",
                "astraZW5GLFefxNPAatceHhYjfA1ciq9gvfEg2S47xk",
                "astrawVNP4xDBKT7rAdxrLYiTSTdqtUr63fSMduivXK",
            ],
            Processor::Blockrazor => &[
                "FjmZZrFvhnqqb9ThCuMVnENaM3JGVuGWNyCAxRJcFpg9",
                "6No2i3aawzHsjtThw81iq1EXPJN6rh8eSJCLaYZfKDTG",
                "A9cWowVAiHe9pJfKAj3TJiN9VpbzMUq6E4kEvf5mUT22",
                "Gywj98ophM7GmkDdaWs4isqZnDdFCW7B46TXmKfvyqSm",
                "68Pwb4jS7eZATjDfhmTXgRJjCiZmw1L7Huy4HNpnxJ3o",
                "4ABhJh5rZPjv63RBJBuyWzBK3g9gWMUQdTZP2kiW31V9",
                "B2M4NG5eyZp5SBQrSdtemzk5TqVuaWGQnowGaCBt8GyM",
                "5jA59cXMKQqZAVdtopv8q3yyw9SYfiE3vUCbt7p8MfVf",
                "5YktoWygr1Bp9wiS1xtMtUki1PeYuuzuCF98tqwYxf61",
                "295Avbam4qGShBYK7E9H5Ldew4B3WyJGmgmXfiWdeeyV",
                "EDi4rSy2LZgKJX74mbLTFk4mxoTgT6F7HxxzG2HBAFyK",
                "BnGKHAC386n4Qmv9xtpBVbRaUTKixjBe3oagkPFKtoy6",
                "Dd7K2Fp7AtoN8xCghKDRmyqr5U169t48Tw5fEd3wT9mq",
                "AP6qExwrbRgBAVaehg4b5xHENX815sMabtBzUzVB4v8S",
            ],
        }
    }
}
