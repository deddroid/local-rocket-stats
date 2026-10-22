//! Readable names for playlists and arenas.

/// Playlist ids reported by the game (`Game.PlaylistId`) and their names.
pub const PLAYLISTS: &[(i64, &str)] = &[
    (1, "Duel"),
    (2, "Doubles"),
    (3, "Standard"),
    (4, "Chaos"),
    (6, "Private Match"),
    (7, "Season"),
    (8, "Offline"),
    (9, "Training"),
    (10, "Ranked Duel 1v1"),
    (11, "Ranked Doubles 2v2"),
    (13, "Ranked Standard 3v3"),
    (15, "Snow Day"),
    (16, "Rocket Labs"),
    (17, "Hoops"),
    (18, "Rumble"),
    (19, "Workshop"),
    (22, "Custom Tournament"),
    (23, "Dropshot"),
    (24, "Local Match"),
    (26, "FaceIt"),
    (27, "Ranked Hoops"),
    (28, "Ranked Rumble"),
    (29, "Ranked Dropshot"),
    (30, "Ranked Snow Day"),
    (31, "Ghost Hunt"),
    (32, "Beach Ball"),
    (33, "Spike Rush"),
    (34, "Tournament"),
    (35, "Rocket Labs"),
    (37, "Dropshot Rumble"),
    (38, "Heatseeker"),
    (41, "Boomer Ball"),
    (43, "Heatseeker Doubles"),
    (44, "Winter Breakaway"),
    (46, "Gridiron"),
    (47, "Super Cube"),
    (48, "Tactical Rumble"),
    (49, "Spring Loaded"),
    (50, "Speed Demon"),
    (52, "Gotham City Rumble"),
    (54, "Knockout"),
];

/// Competitive playlists, where the game shows MMR after the match.
pub const RANKED: &[i64] = &[10, 11, 13, 27, 28, 29, 30];

/// Matches not worth recording (training, workshop maps).
pub const IGNORED: &[i64] = &[9, 19];

/// Arena asset names (lowercase) and their names.
pub const ARENAS: &[(&str, &str)] = &[
    ("stadium_p", "DFH Stadium"),
    ("stadium_day_p", "DFH Stadium (Day)"),
    ("stadium_foggy_p", "DFH Stadium (Stormy)"),
    ("stadium_winter_p", "DFH Stadium (Snowy)"),
    ("eurostadium_p", "Mannfield"),
    ("eurostadium_night_p", "Mannfield (Night)"),
    ("eurostadium_rainy_p", "Mannfield (Stormy)"),
    ("eurostadium_snownight_p", "Mannfield (Snowy)"),
    ("cs_p", "Champions Field"),
    ("cs_day_p", "Champions Field (Day)"),
    ("cs_hw_p", "Rivals Arena"),
    ("utopiastadium_p", "Utopia Coliseum"),
    ("utopiastadium_dusk_p", "Utopia Coliseum (Dusk)"),
    ("utopiastadium_snow_p", "Utopia Coliseum (Snowy)"),
    ("trainstation_p", "Urban Central"),
    ("trainstation_night_p", "Urban Central (Night)"),
    ("trainstation_dawn_p", "Urban Central (Dawn)"),
    ("park_p", "Beckwith Park"),
    ("park_night_p", "Beckwith Park (Midnight)"),
    ("park_rainy_p", "Beckwith Park (Stormy)"),
    ("tokyounderpass_p", "Neo Tokyo"),
    ("neotokyo_standard_p", "Neo Tokyo"),
    ("neotokyo_arcade_p", "Neo Tokyo (Arcade)"),
    ("farm_p", "Farmstead"),
    ("farm_night_p", "Farmstead (Night)"),
    ("wasteland_p", "Wasteland"),
    ("wasteland_s_p", "Wasteland"),
    ("wasteland_night_s_p", "Wasteland (Night)"),
    ("aquadome_p", "AquaDome"),
    ("underwater_p", "AquaDome"),
    ("beach_p", "Salty Shores"),
    ("beach_night_p", "Salty Shores (Night)"),
    ("music_p", "Neon Fields"),
    ("street_p", "Sovereign Heights"),
    ("chn_stadium_p", "Forbidden Temple"),
    ("chn_stadium_day_p", "Forbidden Temple (Day)"),
    ("arc_standard_p", "Starbase ARC"),
    ("outlaw_p", "Deadeye Canyon"),
    ("outlaw_oasis_p", "Deadeye Canyon (Oasis)"),
    ("ff_dusk_p", "Estadio Vida"),
    ("swoosh_p", "Champions Field (Nike FC)"),
    ("hoopsstadium_p", "Dunk House"),
    ("shattershot_p", "Core 707"),
    ("labs_utopia_p", "Throwback Stadium"),
    ("fni_stadium_p", "Futura Garden"),
];

pub fn is_ranked(playlist: Option<i64>) -> bool {
    playlist.is_some_and(|p| RANKED.contains(&p))
}

pub fn is_ignored(playlist: Option<i64>) -> bool {
    playlist.is_some_and(|p| IGNORED.contains(&p))
}

pub fn playlist_name(id: Option<i64>) -> String {
    match id {
        None => "Unknown".into(),
        Some(id) => PLAYLISTS
            .iter()
            .find(|(k, _)| *k == id)
            .map(|(_, n)| (*n).to_string())
            .unwrap_or_else(|| format!("Playlist {id}")),
    }
}

pub fn arena_name(asset: Option<&str>) -> String {
    let Some(asset) = asset.filter(|a| !a.is_empty()) else {
        return String::new();
    };
    let key = asset.to_lowercase();
    if let Some((_, name)) = ARENAS.iter().find(|(k, _)| *k == key) {
        return (*name).to_string();
    }
    key.strip_suffix("_p")
        .unwrap_or(&key)
        .split('_')
        .map(|w| {
            let mut c = w.chars();
            c.next()
                .map(|f| f.to_uppercase().chain(c).collect::<String>())
                .unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names() {
        assert_eq!(playlist_name(Some(11)), "Ranked Doubles 2v2");
        assert_eq!(playlist_name(Some(999)), "Playlist 999");
        assert_eq!(arena_name(Some("Stadium_P")), "DFH Stadium");
        assert_eq!(arena_name(Some("Paname_Dusk_P")), "Paname Dusk");
        assert!(is_ranked(Some(13)) && !is_ranked(Some(8)));
    }
}
