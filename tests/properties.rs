use proptest::prelude::*;
use vutils::codec;

proptest! {
    #[test]
    fn binary_codecs_round_trip(input in proptest::collection::vec(any::<u8>(), 0..4096)) {
        for (url_safe, padding) in [(false, false), (false, true), (true, false), (true, true)] {
            let encoded = codec::base64_encode(&input, url_safe, padding);
            let decoded = codec::base64_decode(&encoded, url_safe, padding).unwrap();
            prop_assert_eq!(decoded.as_slice(), input.as_slice());
        }

        let decoded_hex = codec::hex_decode(&codec::hex_encode(&input, false)).unwrap();
        prop_assert_eq!(decoded_hex.as_slice(), input.as_slice());
        prop_assert_eq!(
            codec::gzip_decompress(&codec::gzip_compress(&input, 6).unwrap()).unwrap(),
            input
        );
    }

    #[test]
    fn url_component_round_trip(input in ".{0,2048}") {
        let encoded = codec::url_encode(&input, false);
        prop_assert_eq!(codec::url_decode(&encoded, false).unwrap(), input);
    }

    #[test]
    fn form_url_round_trip(input in "[^+]{0,2048}") {
        let encoded = codec::url_encode(&input, true);
        prop_assert_eq!(codec::url_decode(&encoded, true).unwrap(), input);
    }
}
