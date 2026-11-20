use mongodb::{
    Client,
    bson::{Document, doc},
    event::{EventHandler, command::CommandEvent},
    options::{ClientOptions, ReadPreference, SelectionCriteria},
};
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::time::{Instant, sleep, timeout};

type Commands = Arc<Mutex<Vec<(String, String)>>>;

fn secondary() -> SelectionCriteria {
    ReadPreference::secondary(None).into()
}

#[tokio::main]
async fn main() -> mongodb::error::Result<()> {
    let uri = "mongodb://mongo1:27017,mongo2:27017/?replicaSet=rs0&serverSelectionTimeoutMS=10000";

    // Seed and confirm replication without adding commands to the monitored log.
    let seed_client = Client::with_uri_str(uri).await?;
    let collection = seed_client
        .database("repro")
        .collection::<Document>("items");
    collection.delete_many(doc! {}).await?;
    collection
        .insert_many(
            (0..6)
                .map(|i| doc! {"_id": i, "value": i})
                .collect::<Vec<_>>(),
        )
        .await?;
    timeout(Duration::from_secs(15), async {
        loop {
            if collection
                .count_documents(doc! {})
                .selection_criteria(secondary())
                .await
                .unwrap_or(0)
                == 6
            {
                break;
            }
            sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .expect("timed out waiting for all six documents on secondary");
    drop(seed_client);

    let commands: Commands = Arc::new(Mutex::new(Vec::new()));
    let captured = commands.clone();
    let mut options = ClientOptions::parse(uri).await?;
    options.command_event_handler = Some(EventHandler::callback(move |event: CommandEvent| {
        if let CommandEvent::Started(event) = event {
            if ["aggregate", "getMore", "killCursors"].contains(&event.command_name.as_str()) {
                let address = event.connection.address.to_string();
                println!("{} -> {}", event.command_name, address);
                captured.lock().unwrap().push((event.command_name, address));
            }
        }
    }));
    let client = Client::with_options(options)?;
    let coll = client.database("repro").collection::<Document>("items");
    let mut cursor = coll
        .aggregate(vec![doc! {"$match": {}}])
        .selection_criteria(secondary())
        .batch_size(2)
        .await?;
    for _ in 0..3 {
        assert!(
            cursor.advance().await?,
            "expected a row before dropping the cursor"
        );
    }
    drop(cursor);

    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if commands
            .lock()
            .unwrap()
            .iter()
            .any(|(name, _)| name == "killCursors")
        {
            break;
        }
        if Instant::now() >= deadline {
            eprintln!("timed out waiting for killCursors cleanup event");
            std::process::exit(2);
        }
        sleep(Duration::from_millis(25)).await;
    }
    let logged = commands.lock().unwrap();
    let aggregate = logged.iter().find(|(name, _)| name == "aggregate");
    let get_more = logged.iter().find(|(name, _)| name == "getMore");
    let kill = logged.iter().find(|(name, _)| name == "killCursors");
    let (Some(aggregate), Some(get_more), Some(kill)) = (aggregate, get_more, kill) else {
        eprintln!("aggregate, getMore, or killCursors event missing");
        std::process::exit(2);
    };
    if aggregate.1 != "mongo2:27017" || get_more.1 != aggregate.1 {
        eprintln!("the cursor must open and fetch from the fixture secondary");
        std::process::exit(2);
    }
    println!("expected cleanup destination: {}", aggregate.1);
    println!("actual cleanup destination: {}", kill.1);
    if aggregate.1 != kill.1 {
        eprintln!(
            "BUG reproduced: aggregate={} killCursors={}",
            aggregate.1, kill.1
        );
        std::process::exit(1);
    }
    println!("cleanup matched");
    Ok(())
}
