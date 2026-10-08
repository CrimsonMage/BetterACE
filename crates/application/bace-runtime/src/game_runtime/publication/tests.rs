use super::*;
use bace_storage_codec::{PackKey, PackLookup};
async fn insert(runtime: &GameRuntime) -> i64 {
    runtime.bootstrap.store.insert_candidates(&[bace_persistence::ContentCandidate {
        wcid: 2, class_name: "published_creature".into(), weenie_type: 10,
        bytes: bace_content_tools::compile("schema_version=1\nweenie_id=2\nclass_name=\"published_creature\"\nweenie_type=10\n").unwrap(),
    }]).await.unwrap()
}
async fn drive(runtime: &mut GameRuntime, now: Duration) {
    for _ in 0..1000 {
        runtime.poll_publication(now).unwrap();
        if !runtime.publication.has_pending() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(1)).await;
    }
    panic!("publication did not finish");
}
#[tokio::test]
async fn accepted_publication_reaches_future_region_inputs_without_mutating_old_reader() {
    let (_cluster, _directory, mut runtime) = crate::game_runtime::tests::fixture::fixture().await;
    let old = runtime.bootstrap.pack.generation.clone();
    let revision = insert(&runtime).await;
    drive(&mut runtime, Duration::ZERO).await;
    assert_eq!(
        runtime
            .bootstrap
            .store
            .active_generation()
            .await
            .unwrap()
            .unwrap()
            .accepted_revision,
        revision
    );
    assert!(matches!(
        runtime
            .bootstrap
            .pack
            .generation
            .lookup(PackKey {
                namespace: 1,
                id: 2
            })
            .unwrap(),
        PackLookup::Record(_)
    ));
    assert!(matches!(
        old.lookup(PackKey {
            namespace: 1,
            id: 2
        })
        .unwrap(),
        PackLookup::Missing
    ));
    assert_eq!(
        runtime.world.as_ref().unwrap().regions.content_generation(),
        runtime.bootstrap.pack.generation.revision()
    );
    assert!(runtime.bootstrap.pack_io.is_some());
    assert!(runtime.publication.failure.is_none());
}
#[tokio::test]
async fn lost_postcommit_delivery_reloads_durable_authority_before_retry() {
    let (_cluster, _directory, mut runtime) = crate::game_runtime::tests::fixture::fixture().await;
    let revision = insert(&runtime).await;
    let mut worker = runtime.bootstrap.pack_io.take().unwrap();
    let mut delivered = PreparedPack {
        manifest: runtime.bootstrap.pack.manifest.clone(),
        generation: runtime.bootstrap.pack.generation.clone(),
    };
    let (sender, _receiver) = tokio::sync::mpsc::channel(1);
    assert_eq!(
        crate::native_publication::publish_native_once(
            &runtime.bootstrap.store,
            &mut worker,
            &mut delivered,
            &sender
        )
        .await
        .unwrap(),
        crate::PublicationResult::Accepted { revision }
    );
    runtime.bootstrap.pack_io = Some(worker);
    // Simulate losing adoption after the database committed. The retained live
    // owner has the old pack, while durable authority and candidate heads agree.
    let accepted_hash = delivered.manifest.content_hash(Default::default()).unwrap();
    let mut failed = false;
    for _ in 0..1000 {
        if runtime.poll_publication(Duration::ZERO).is_err() {
            failed = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(1)).await;
    }
    assert!(failed);
    assert!(runtime.publication.recover && runtime.publication.has_pending());
    drive(&mut runtime, Duration::from_secs(2)).await;
    assert_eq!(
        runtime
            .bootstrap
            .pack
            .manifest
            .content_hash(Default::default())
            .unwrap(),
        accepted_hash
    );
    assert_eq!(
        runtime.bootstrap.store.pending_revision().await.unwrap(),
        None
    );
    assert!(runtime.publication.failure.is_none());
    assert!(runtime.bootstrap.pack_io.is_some());
}

#[tokio::test]
#[ignore = "requires approved DATs, accepted full native pack and PostgreSQL binaries"]
async fn occupied_client_observes_published_static_weenie_add_and_remove() {
    use bace_content::WorldRecordV1;
    use bace_persistence::{ContentCandidate, MappedContentCandidate};
    let mut fixture = Box::pin(crate::game_runtime::tests::entered::fixture()).await;
    let block = 0x8602;
    let region = fixture
        .runtime
        .world
        .as_ref()
        .unwrap()
        .regions
        .prepared_region(block)
        .expect("entered player's resident region")
        .clone();
    let source = region
        .content
        .instances
        .iter()
        .find(|instance| {
            !instance.source.is_link_child
                && instance.template.weenie_type != 12
                && !crate::generator_preparation::is_creature_template(
                    instance.template.weenie_type,
                )
                && instance.template.properties.generators.is_empty()
                && instance.template.properties.create_list.is_empty()
                && region
                    .physical
                    .get(&instance.template.weenie_id)
                    .is_some_and(Result::is_ok)
                && fixture.client.has_create(instance.source.guid, 0)
        })
        .expect("visible plain authored source");
    let new_template_id = 5_000_001;
    let new_guid = 0x7e00_0001;
    assert!(matches!(
        fixture
            .runtime
            .bootstrap
            .pack
            .generation
            .lookup(PackKey {
                namespace: 1,
                id: new_template_id,
            })
            .unwrap(),
        PackLookup::Missing
    ));
    assert!(matches!(
        fixture
            .runtime
            .bootstrap
            .pack
            .generation
            .lookup(PackKey {
                namespace: 20,
                id: new_guid,
            })
            .unwrap(),
        PackLookup::Missing
    ));
    let mut weenie = (*source.template).clone();
    weenie.weenie_id = new_template_id as u32;
    weenie.class_name = "live_published_static_fixture".into();
    if let Some(name) = weenie
        .properties
        .strings
        .iter_mut()
        .find(|property| property.id == 1)
    {
        name.value = "live_published_static_fixture".into();
    } else {
        weenie.properties.strings.push(bace_content::Property {
            id: 1,
            value: "live_published_static_fixture".into(),
        });
    }
    weenie
        .properties
        .strings
        .sort_unstable_by_key(|property| property.id);
    let mut row = source.source.clone();
    row.guid = new_guid as u32;
    row.weenie_class_id = new_template_id as u32;
    row.origin_x += 1.5;
    row.origin_y += 1.5;
    let old_hash = fixture
        .runtime
        .bootstrap
        .pack
        .manifest
        .content_hash(Default::default())
        .unwrap();
    fixture
        .runtime
        .bootstrap
        .store
        .insert_mapped_batch(
            &[ContentCandidate {
                wcid: new_template_id as u32,
                class_name: weenie.class_name.clone(),
                weenie_type: weenie.weenie_type as i32,
                bytes: bace_content_tools::compile(&bace_content_tools::export(&weenie).unwrap())
                    .unwrap(),
            }],
            &[],
            &[MappedContentCandidate {
                namespace: 20,
                id: new_guid,
                schema: 1,
                bytes: Some(
                    bace_content_tools::compile_world_record(&WorldRecordV1::LandblockInstance(
                        row,
                    ))
                    .unwrap(),
                ),
            }],
            old_hash,
        )
        .await
        .unwrap();
    let from = fixture.client.messages.len();
    crate::game_runtime::tests::entered::poll_until(
        &mut fixture.runtime,
        &mut fixture.client,
        |runtime, client| {
            runtime.world.as_ref().is_some_and(|world| {
                world.regions.content_generation() > region.fence.content_generation
            }) && client.has_create(new_guid as u32, from)
        },
    )
    .await;
    assert_eq!(
        fixture
            .runtime
            .visibility
            .service
            .registered_object_name(bace_types::EntityId(new_guid as u32)),
        Some("live_published_static_fixture")
    );
    let accepted = fixture
        .runtime
        .bootstrap
        .pack
        .manifest
        .content_hash(Default::default())
        .unwrap();
    fixture
        .runtime
        .bootstrap
        .store
        .insert_mapped_batch(
            &[],
            &[],
            &[
                MappedContentCandidate {
                    namespace: 20,
                    id: new_guid,
                    schema: 1,
                    bytes: None,
                },
                MappedContentCandidate {
                    namespace: 1,
                    id: new_template_id,
                    schema: 1,
                    bytes: None,
                },
            ],
            accepted,
        )
        .await
        .unwrap();
    let from = fixture.client.messages.len();
    crate::game_runtime::tests::entered::poll_until(
        &mut fixture.runtime,
        &mut fixture.client,
        |_, client| client.has_delete(new_guid as u32, from),
    )
    .await;
    assert!(
        fixture
            .runtime
            .visibility
            .service
            .registered_object_name(bace_types::EntityId(new_guid as u32))
            .is_none()
    );
    fixture.shutdown().await;
}
