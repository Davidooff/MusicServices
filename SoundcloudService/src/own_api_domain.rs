struct IdNameGroup{
    id: String,
    name: String,
}

struct TrackImage{
    url: String,
    w: u16,
    h: u16,
}

struct TrackData {
    id: String,
    name: String,
    id_name_group: IdNameGroup,
    eplatform: u8,
    img_urls: TrackImage,
    album_id: String,
    duration: u16,
}
