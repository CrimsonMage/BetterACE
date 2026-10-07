use bace_transport::*;

#[test]
fn ui_queue_blocks_overtaking_while_other_queues_fill_gaps() {
    for queue in [3, 9] {
        let mut bundle = Bundler::new(10, 10000, 10000);
        bundle.enqueue(queue, vec![0; 400]).unwrap();
        bundle.enqueue(queue, vec![1; 100]).unwrap();
        bundle.enqueue(queue, vec![2; 4]).unwrap();
        bundle.start_bundle().unwrap();
        let parts = bundle.next_fragments();
        assert_eq!(
            parts.iter().map(|f| f.header.sequence).collect::<Vec<_>>(),
            if queue == 9 { vec![0] } else { vec![0, 2] }
        );
    }
}
#[test]
fn tails_pack_before_bodies_and_exact_multiple_tails_stay_bounded() {
    for length in [448, 449, 463, 464, 896, 900] {
        let mut bundle = Bundler::new(10, 10000, 10000);
        bundle.enqueue(3, vec![0; 4]).unwrap();
        bundle.enqueue(3, vec![1; length]).unwrap();
        bundle.start_bundle().unwrap();
        let mut parts = Vec::new();
        while bundle.has_active_bundle() {
            let packet = bundle.next_fragments();
            assert!(!packet.is_empty());
            assert!(
                packet
                    .iter()
                    .map(|f| usize::from(f.header.size))
                    .sum::<usize>()
                    <= 464
            );
            parts.extend(packet);
        }
        let mut assembly = Reassembly::new(ReassemblyLimits::default());
        let mut actual = None;
        for part in parts.into_iter().filter(|f| f.header.sequence == 1) {
            actual = assembly.insert(part, 0).unwrap().or(actual);
        }
        assert_eq!(actual, Some(vec![1; length]));
        assert_eq!(bundle.buffered_bytes(), 0);
        assert_eq!(bundle.pending_messages(), 0);
    }
}
#[test]
fn rotating_queue_service_and_message_sequence_assignment() {
    let mut bundle = Bundler::new(10, 10000, 10000);
    bundle.enqueue(9, vec![9; 4]).unwrap();
    bundle.enqueue(1, vec![1; 4]).unwrap();
    bundle.start_bundle().unwrap();
    let first = bundle.next_fragments();
    assert_eq!((first[0].header.queue, first[0].header.sequence), (1, 0));
    bundle.enqueue(1, vec![2; 4]).unwrap();
    bundle.start_bundle().unwrap();
    let second = bundle.next_fragments();
    assert_eq!((second[0].header.queue, second[0].header.sequence), (9, 1));
}
