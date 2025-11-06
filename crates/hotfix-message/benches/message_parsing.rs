use criterion::{Criterion, criterion_group, criterion_main};
use hotfix_dictionary::Dictionary;
use hotfix_message::MessageBuilder;
use hotfix_message::message::Config;
use std::hint::black_box;

const EXECUTION_REPORT_MESSAGE: &[u8] = b"8=FIX.4.4|9=247|35=8|34=2|49=Broker|52=20231103-09:30:00|56=Client|11=Order12345|17=Exec12345|150=0|39=0|55=APPL|54=1|38=100|32=50|31=150.00|151=50|14=50|6=150.00|453=2|448=PARTYA|447=D|452=1|802=2|523=SUBPARTYA1|803=1|523=SUBPARTYA2|803=2|448=PARTYB|447=D|452=2|10=129|";
const CONFIG: Config = Config::with_separator(b'|');

fn parse_message(builder: &MessageBuilder) {
    let parsed = builder.build(black_box(EXECUTION_REPORT_MESSAGE));
    black_box(parsed);
}

fn message_parsing_benchmark(c: &mut Criterion) {
    let dict = Dictionary::fix44();
    let builder = MessageBuilder::new(dict.clone(), CONFIG).unwrap();
    c.bench_function("parse message", |b| {
        b.iter(|| parse_message(black_box(&builder)))
    });
}

criterion_group!(benches, message_parsing_benchmark);
criterion_main!(benches);
