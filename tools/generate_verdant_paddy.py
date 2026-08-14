#!/usr/bin/env python3
"""Build the deterministic Tiled authoring package for Verdant Paddy."""

from __future__ import annotations

import json
from pathlib import Path

MAP_SIZE = 64
TILE_SIZE = 32

GID = {
    "grass": 1,
    "flowers": 2,
    "weeds": 3,
    "rock": 4,
    "soil": 5,
    "mud": 6,
    "plowed": 7,
    "tilled": 8,
    "road": 9,
    "path": 10,
    "concrete": 11,
    "culvert": 12,
    "channel": 13,
    "river": 14,
    "bund": 15,
    "entrance": 16,
}

FACILITY_GID = {
    "RobotGarage": 25,
    "Warehouse": 26,
    "ChargingStation": 27,
    "ShippingDock": 28,
    "Packer": 29,
    "SolarGenerator": 30,
    "Battery": 31,
    "WaterPump": 32,
    "IrrigationNode": 33,
}

INFRASTRUCTURE_GID = {
    "bund_horizontal": 17,
    "bund_vertical": 18,
    "bund_corner": 19,
    "farm_path": 20,
    "channel_horizontal": 21,
    "channel_vertical": 22,
    "culvert": 23,
    "garage_apron": 24,
}

TILED_FLIP_HORIZONTAL = 0x80000000
TILED_FLIP_VERTICAL = 0x40000000
TILED_FLIP_DIAGONAL = 0x20000000
ROTATE_90 = TILED_FLIP_HORIZONTAL | TILED_FLIP_DIAGONAL
ROTATE_180 = TILED_FLIP_HORIZONTAL | TILED_FLIP_VERTICAL
ROTATE_270 = TILED_FLIP_VERTICAL | TILED_FLIP_DIAGONAL


def empty_layer() -> list[int]:
    return [0] * (MAP_SIZE * MAP_SIZE)


def put(layer: list[int], x: int, y: int, gid: int) -> None:
    if 0 <= x < MAP_SIZE and 0 <= y < MAP_SIZE:
        layer[y * MAP_SIZE + x] = gid


def fill(layer: list[int], x: int, y: int, width: int, height: int, gid: int) -> None:
    for tile_y in range(y, y + height):
        for tile_x in range(x, x + width):
            put(layer, tile_x, tile_y, gid)


def rectangle_border(
    layer: list[int], x: int, y: int, width: int, height: int, gid: int
) -> None:
    fill(layer, x, y, width, 1, gid)
    fill(layer, x, y + height - 1, width, 1, gid)
    fill(layer, x, y, 1, height, gid)
    fill(layer, x + width - 1, y, 1, height, gid)


def paddy_bund(layer: list[int], x: int, y: int, width: int, height: int) -> None:
    fill(layer, x + 1, y, width - 2, 1, INFRASTRUCTURE_GID["bund_horizontal"])
    fill(layer, x + 1, y + height - 1, width - 2, 1, INFRASTRUCTURE_GID["bund_horizontal"])
    fill(layer, x, y + 1, 1, height - 2, INFRASTRUCTURE_GID["bund_vertical"])
    fill(layer, x + width - 1, y + 1, 1, height - 2, INFRASTRUCTURE_GID["bund_vertical"])
    put(layer, x, y, INFRASTRUCTURE_GID["bund_corner"])
    put(layer, x + width - 1, y, INFRASTRUCTURE_GID["bund_corner"] | ROTATE_90)
    put(layer, x + width - 1, y + height - 1, INFRASTRUCTURE_GID["bund_corner"] | ROTATE_180)
    put(layer, x, y + height - 1, INFRASTRUCTURE_GID["bund_corner"] | ROTATE_270)


def property_value(name: str, value: object, kind: str | None = None) -> dict[str, object]:
    property_kind = kind
    if property_kind is None:
        property_kind = "bool" if isinstance(value, bool) else "int" if isinstance(value, int) else "string"
    return {"name": name, "type": property_kind, "value": value}


def tile_layer(layer_id: int, name: str, data: list[int], z: int, logical: bool) -> dict[str, object]:
    return {
        "id": layer_id,
        "name": name,
        "type": "tilelayer",
        "width": MAP_SIZE,
        "height": MAP_SIZE,
        "x": 0,
        "y": 0,
        "opacity": 1,
        "visible": True,
        "data": data,
        "properties": [
            property_value("render_z", z),
            property_value("simulation_terrain", logical),
        ],
    }


def object_layer(layer_id: int, name: str, objects: list[dict[str, object]], color: str) -> dict[str, object]:
    return {
        "id": layer_id,
        "name": name,
        "type": "objectgroup",
        "draworder": "topdown",
        "opacity": 1,
        "visible": True,
        "color": color,
        "objects": objects,
    }


def point_object(
    object_id: int,
    name: str,
    class_name: str,
    x: float,
    y: float,
    properties: list[dict[str, object]] | None = None,
) -> dict[str, object]:
    return {
        "id": object_id,
        "name": name,
        "class": class_name,
        "point": True,
        "x": x * TILE_SIZE,
        "y": y * TILE_SIZE,
        "width": 0,
        "height": 0,
        "rotation": 0,
        "visible": True,
        "properties": properties or [],
    }


def rectangle_object(
    object_id: int,
    name: str,
    class_name: str,
    x: int,
    y: int,
    width: int,
    height: int,
    properties: list[dict[str, object]] | None = None,
) -> dict[str, object]:
    return {
        "id": object_id,
        "name": name,
        "class": class_name,
        "x": x * TILE_SIZE,
        "y": y * TILE_SIZE,
        "width": width * TILE_SIZE,
        "height": height * TILE_SIZE,
        "rotation": 0,
        "visible": True,
        "properties": properties or [],
    }


def facility_object(
    object_id: int,
    kind: str,
    anchor: tuple[int, int],
    origin: tuple[int, int],
    size: tuple[int, int],
) -> dict[str, object]:
    return {
        **rectangle_object(
            object_id,
            kind,
            "Facility",
            origin[0],
            origin[1],
            size[0],
            size[1],
            [
                property_value("kind", kind),
                property_value("anchor_x", anchor[0]),
                property_value("anchor_y", anchor[1]),
                property_value("atlas_index", FACILITY_GID[kind] - 25),
            ],
        ),
        "gid": FACILITY_GID[kind],
    }


def build_layers() -> tuple[list[dict[str, object]], int, int]:
    ground = [GID["grass"]] * (MAP_SIZE * MAP_SIZE)
    for y in range(MAP_SIZE):
        for x in range(MAP_SIZE):
            if (x * 17 + y * 31) % 23 == 0:
                put(ground, x, y, GID["flowers"])
            elif (x * 13 + y * 7) % 41 == 0:
                put(ground, x, y, GID["weeds"])

    terrain = empty_layer()
    infrastructure = empty_layer()
    detail = empty_layer()

    fill(terrain, 0, 0, MAP_SIZE, 1, GID["rock"])
    fill(terrain, 0, MAP_SIZE - 1, MAP_SIZE, 1, GID["rock"])
    fill(terrain, 0, 0, 1, MAP_SIZE, GID["rock"])
    fill(terrain, MAP_SIZE - 1, 0, 1, MAP_SIZE, GID["rock"])

    fill(terrain, 58, 1, 5, 62, GID["river"])
    fill(terrain, 6, 1, 2, 62, GID["channel"])
    fill(terrain, 6, 10, 33, 2, GID["channel"])
    fill(terrain, 37, 10, 2, 42, GID["channel"])
    fill(terrain, 6, 50, 33, 2, GID["channel"])
    fill(terrain, 28, 50, 2, 13, GID["channel"])

    fill(terrain, 10, 20, 11, 28, GID["soil"])
    fill(terrain, 23, 20, 13, 28, GID["soil"])

    fill(terrain, 39, 5, 17, 12, GID["concrete"])
    fill(terrain, 40, 18, 17, 33, GID["concrete"])
    fill(terrain, 0, 14, 58, 3, GID["road"])
    fill(terrain, 40, 29, 17, 2, GID["path"])
    fill(terrain, 47, 18, 2, 33, GID["path"])

    paddy_bund(infrastructure, 9, 19, 13, 30)
    paddy_bund(infrastructure, 22, 19, 15, 30)
    fill(infrastructure, 14, 19, 3, 1, INFRASTRUCTURE_GID["farm_path"])
    fill(infrastructure, 28, 19, 3, 1, INFRASTRUCTURE_GID["farm_path"])
    fill(infrastructure, 6, 14, 2, 3, INFRASTRUCTURE_GID["culvert"])
    fill(infrastructure, 37, 14, 2, 3, INFRASTRUCTURE_GID["culvert"])

    for x, y in [(3, 5), (12, 7), (21, 4), (33, 6), (4, 55), (18, 57), (44, 56), (54, 54)]:
        put(detail, x, y, GID["rock"])
    for x, y in [(2, 9), (16, 3), (27, 8), (51, 3), (3, 36), (52, 57), (41, 58)]:
        put(detail, x, y, GID["flowers"])

    facilities = [
        facility_object(1, "RobotGarage", (47, 12), (40, 5), (15, 9)),
        facility_object(2, "Warehouse", (46, 23), (40, 19), (7, 8)),
        facility_object(3, "ChargingStation", (44, 34), (41, 32), (4, 4)),
        facility_object(4, "ShippingDock", (56, 44), (53, 41), (4, 7)),
        facility_object(5, "Packer", (53, 34), (48, 32), (6, 5)),
        facility_object(6, "SolarGenerator", (45, 43), (40, 40), (6, 5)),
        facility_object(7, "Battery", (52, 43), (48, 40), (5, 5)),
        facility_object(8, "WaterPump", (51, 23), (51, 19), (5, 7)),
        facility_object(9, "IrrigationNode", (44, 49), (41, 47), (4, 4)),
    ]

    zones = [
        rectangle_object(
            10,
            "West Rice Cell",
            "FieldZone",
            10,
            20,
            11,
            28,
            [property_value("crop_id", "rice"), property_value("priority", 85)],
        ),
        rectangle_object(
            11,
            "East Rice Cell",
            "FieldZone",
            23,
            20,
            13,
            28,
            [property_value("crop_id", "rice"), property_value("priority", 82)],
        ),
    ]

    spawns = [
        point_object(12, "Garage Exit", "GarageExit", 46.5, 16.5),
        point_object(13, "Bay 1", "GarageBay", 42.5, 13.5, [property_value("order", 0)]),
        point_object(14, "Bay 2", "GarageBay", 45.5, 13.5, [property_value("order", 1)]),
        point_object(15, "Bay 3", "GarageBay", 48.5, 13.5, [property_value("order", 2)]),
        point_object(16, "Bay 4", "GarageBay", 51.5, 13.5, [property_value("order", 3)]),
        point_object(17, "Paddy Rover", "RobotSpawn", 42.5, 13.5, [property_value("robot_id", "paddy_rover"), property_value("order", 0)]),
        point_object(18, "Rice Transplanter", "RobotSpawn", 45.5, 13.5, [property_value("robot_id", "rice_transplanter"), property_value("order", 1)]),
        point_object(19, "Pest Drone", "RobotSpawn", 48.5, 13.5, [property_value("robot_id", "pest_control_drone"), property_value("order", 2)]),
        point_object(20, "Rice Harvester", "RobotSpawn", 51.5, 13.5, [property_value("robot_id", "rice_harvester"), property_value("order", 3)]),
    ]

    navigation = [
        rectangle_object(21, "Garage Apron", "NavigationArea", 39, 12, 17, 5, [property_value("ground_cost", 1)]),
        rectangle_object(22, "Main Service Road", "NavigationArea", 0, 14, 58, 3, [property_value("ground_cost", 1)]),
        rectangle_object(23, "Facility Service Lane", "NavigationArea", 47, 18, 2, 33, [property_value("ground_cost", 1)]),
        rectangle_object(24, "West Paddy", "NavigationArea", 10, 20, 11, 28, [property_value("legged_cost", 2), property_value("wheeled_empty_only", True)]),
        rectangle_object(25, "East Paddy", "NavigationArea", 23, 20, 13, 28, [property_value("legged_cost", 2), property_value("wheeled_empty_only", True)]),
    ]

    collision = [
        rectangle_object(26, "East River", "CollisionArea", 58, 1, 5, 62, [property_value("blocks", "ground")]),
        rectangle_object(27, "West Main Channel", "CollisionArea", 6, 1, 2, 62, [property_value("blocks", "ground")]),
        rectangle_object(28, "North Feeder", "CollisionArea", 6, 10, 33, 2, [property_value("blocks", "ground")]),
        rectangle_object(29, "East Channel", "CollisionArea", 37, 10, 2, 42, [property_value("blocks", "ground")]),
        rectangle_object(30, "South Drain", "CollisionArea", 6, 50, 33, 2, [property_value("blocks", "ground")]),
        rectangle_object(31, "South Outlet", "CollisionArea", 28, 50, 2, 13, [property_value("blocks", "ground")]),
        rectangle_object(32, "Garage Shell", "CollisionArea", 40, 5, 15, 6, [property_value("blocks", "ground"), property_value("owner", "RobotGarage")]),
        rectangle_object(33, "Warehouse Shell", "CollisionArea", 41, 20, 5, 6, [property_value("blocks", "ground"), property_value("owner", "Warehouse")]),
        rectangle_object(34, "Charging Equipment", "CollisionArea", 42, 33, 2, 2, [property_value("blocks", "ground"), property_value("owner", "ChargingStation")]),
        rectangle_object(35, "Dock Equipment", "CollisionArea", 53, 42, 3, 5, [property_value("blocks", "ground"), property_value("owner", "ShippingDock")]),
        rectangle_object(36, "Packer Equipment", "CollisionArea", 49, 33, 4, 3, [property_value("blocks", "ground"), property_value("owner", "Packer")]),
        rectangle_object(37, "Solar Equipment", "CollisionArea", 41, 41, 4, 3, [property_value("blocks", "ground"), property_value("owner", "SolarGenerator")]),
        rectangle_object(38, "Battery Equipment", "CollisionArea", 49, 41, 3, 3, [property_value("blocks", "ground"), property_value("owner", "Battery")]),
        rectangle_object(39, "Pump Equipment", "CollisionArea", 52, 20, 4, 5, [property_value("blocks", "ground"), property_value("owner", "WaterPump")]),
        rectangle_object(40, "Irrigation Controller", "CollisionArea", 42, 48, 2, 2, [property_value("blocks", "ground"), property_value("owner", "IrrigationNode")]),
    ]

    gameplay_group = {
        "id": 9,
        "name": "Gameplay",
        "type": "group",
        "opacity": 1,
        "visible": True,
        "layers": [
            object_layer(5, "Structures", facilities, "#ff9d38"),
            object_layer(6, "Field Zones", zones, "#48d97c"),
            object_layer(7, "Spawns", spawns, "#45b6ff"),
            object_layer(8, "Navigation", navigation, "#6f73ff"),
            {**object_layer(10, "Collision", collision, "#ff4a52"), "visible": False},
        ],
    }

    layers = [
        tile_layer(1, "Ground", ground, 0, True),
        tile_layer(2, "Terrain", terrain, 1, True),
        tile_layer(3, "Infrastructure", infrastructure, 2, True),
        tile_layer(4, "Decoration", detail, 3, False),
        gameplay_group,
    ]
    return layers, 10, 40


def terrain_tileset() -> dict[str, object]:
    names = [
        ("Grass", "Grass", "base"),
        ("Grass Flowers", "Grass", "flowers"),
        ("Grass Weeds", "Grass", "weeds"),
        ("Rock Boundary", "Rock", "boundary"),
        ("Dry Paddy Soil", "Soil", "dry"),
        ("Wet Paddy Mud", "Soil", "wet"),
        ("Ploughed Soil", "Soil", "plowed"),
        ("Rotary Tilled Soil", "Soil", "tilled"),
        ("Stone Service Road", "FarmPath", "road"),
        ("Dirt Farm Path", "FarmPath", "path"),
        ("Concrete Apron", "Concrete", "apron"),
        ("Culvert Deck", "Culvert", "culvert"),
        ("Irrigation Channel", "IrrigationChannel", "channel"),
        ("River Water", "Water", "river"),
        ("Paddy Bund", "PaddyBund", "bund"),
        ("Field Entrance", "FarmPath", "entrance"),
    ]
    tiles = []
    for tile_id, (name, terrain, variant) in enumerate(names):
        tiles.append(
            {
                "id": tile_id,
                "class": "TerrainTile",
                "properties": [
                    property_value("display_name", name),
                    property_value("terrain", terrain),
                    property_value("variant", variant),
                ],
            }
        )
    return {
        "type": "tileset",
        "tiledversion": "1.12.2",
        "version": "1.10",
        "name": "verdant-paddy-terrain",
        "tilewidth": TILE_SIZE,
        "tileheight": TILE_SIZE,
        "tilecount": 16,
        "columns": 4,
        "image": "../../art/pixel/tilesets/verdant-paddy-terrain.png",
        "imagewidth": 128,
        "imageheight": 128,
        "tiles": tiles,
    }


def facility_tileset() -> dict[str, object]:
    ordered = sorted(FACILITY_GID.items(), key=lambda item: item[1])
    return {
        "type": "tileset",
        "tiledversion": "1.12.2",
        "version": "1.10",
        "name": "verdant-paddy-facilities",
        "tilewidth": 256,
        "tileheight": 256,
        "tilecount": 9,
        "columns": 3,
        "objectalignment": "topleft",
        "image": "../../art/pixel/tilesets/verdant-paddy-facilities.png",
        "imagewidth": 768,
        "imageheight": 768,
        "tiles": [
            {
                "id": gid - 25,
                "class": "FacilityTile",
                "properties": [property_value("kind", kind)],
            }
            for kind, gid in ordered
        ],
    }


def infrastructure_tileset() -> dict[str, object]:
    tiles = [
        ("Bund Horizontal", "PaddyBund"),
        ("Bund Vertical", "PaddyBund"),
        ("Bund Corner", "PaddyBund"),
        ("Farm Path", "FarmPath"),
        ("Channel Horizontal", "IrrigationChannel"),
        ("Channel Vertical", "IrrigationChannel"),
        ("Culvert", "Culvert"),
        ("Garage Apron", "GarageApron"),
    ]
    return {
        "type": "tileset",
        "tiledversion": "1.12.2",
        "version": "1.10",
        "name": "verdant-paddy-infrastructure",
        "tilewidth": TILE_SIZE,
        "tileheight": TILE_SIZE,
        "tilecount": 8,
        "columns": 4,
        "image": "../../art/pixel/tilesets/verdant-paddy-infrastructure.png",
        "imagewidth": 128,
        "imageheight": 64,
        "tiles": [
            {
                "id": tile_id,
                "class": "TerrainTile",
                "properties": [
                    property_value("display_name", name),
                    property_value("terrain", terrain),
                ],
            }
            for tile_id, (name, terrain) in enumerate(tiles)
        ],
    }


def main() -> None:
    project_root = Path(__file__).resolve().parents[1]
    maps_root = project_root / "assets" / "maps"
    map_dir = maps_root / "verdant-paddy"
    tileset_dir = maps_root / "tilesets"
    map_dir.mkdir(parents=True, exist_ok=True)
    tileset_dir.mkdir(parents=True, exist_ok=True)

    layers, next_layer_id, next_object_id = build_layers()
    tiled_map = {
        "type": "map",
        "tiledversion": "1.12.2",
        "version": "1.10",
        "orientation": "orthogonal",
        "renderorder": "right-down",
        "infinite": False,
        "width": MAP_SIZE,
        "height": MAP_SIZE,
        "tilewidth": TILE_SIZE,
        "tileheight": TILE_SIZE,
        "nextlayerid": next_layer_id + 1,
        "nextobjectid": next_object_id + 1,
        "backgroundcolor": "#17261b",
        "properties": [
            property_value("map_id", "verdant-paddy"),
            property_value("display_name", "Verdant Autonomous Paddy"),
            property_value("schema_version", 1),
        ],
        "tilesets": [
            {"firstgid": 1, "source": "../tilesets/verdant-paddy-terrain.tsj"},
            {"firstgid": 17, "source": "../tilesets/verdant-paddy-infrastructure.tsj"},
            {"firstgid": 25, "source": "../tilesets/verdant-paddy-facilities.tsj"},
        ],
        "layers": layers,
    }

    project = {
        "folders": ["."],
        "propertyTypes": [
            {
                "name": "TerrainTile",
                "type": "class",
                "members": [
                    {"name": "terrain", "type": "string", "value": "Grass"},
                    {"name": "variant", "type": "string", "value": "base"},
                ],
            },
            {
                "name": "Facility",
                "type": "class",
                "members": [
                    {"name": "kind", "type": "string", "value": "Warehouse"},
                    {"name": "anchor_x", "type": "int", "value": 0},
                    {"name": "anchor_y", "type": "int", "value": 0},
                    {"name": "atlas_index", "type": "int", "value": 0},
                ],
            },
            {
                "name": "FacilityTile",
                "type": "class",
                "members": [
                    {"name": "kind", "type": "string", "value": "Warehouse"},
                ],
            },
            {
                "name": "FieldZone",
                "type": "class",
                "members": [
                    {"name": "crop_id", "type": "string", "value": "rice"},
                    {"name": "priority", "type": "int", "value": 60},
                ],
            },
            {"name": "GarageExit", "type": "class", "members": []},
            {
                "name": "GarageBay",
                "type": "class",
                "members": [{"name": "order", "type": "int", "value": 0}],
            },
            {
                "name": "RobotSpawn",
                "type": "class",
                "members": [
                    {"name": "robot_id", "type": "string", "value": "paddy_rover"},
                    {"name": "order", "type": "int", "value": 0},
                ],
            },
            {
                "name": "NavigationArea",
                "type": "class",
                "members": [
                    {"name": "ground_cost", "type": "int", "value": 1},
                    {"name": "legged_cost", "type": "int", "value": 1},
                    {"name": "wheeled_empty_only", "type": "bool", "value": False},
                ],
            },
            {
                "name": "CollisionArea",
                "type": "class",
                "members": [
                    {"name": "blocks", "type": "string", "value": "ground"},
                    {"name": "owner", "type": "string", "value": ""},
                ],
            },
        ],
    }

    (map_dir / "verdant-paddy.tmj").write_text(
        json.dumps(tiled_map, indent=2) + "\n", encoding="utf-8"
    )
    (maps_root / "autofarm.tiled-project").write_text(
        json.dumps(project, indent=2) + "\n", encoding="utf-8"
    )
    (tileset_dir / "verdant-paddy-terrain.tsj").write_text(
        json.dumps(terrain_tileset(), indent=2) + "\n", encoding="utf-8"
    )
    (tileset_dir / "verdant-paddy-facilities.tsj").write_text(
        json.dumps(facility_tileset(), indent=2) + "\n", encoding="utf-8"
    )
    (tileset_dir / "verdant-paddy-infrastructure.tsj").write_text(
        json.dumps(infrastructure_tileset(), indent=2) + "\n", encoding="utf-8"
    )


if __name__ == "__main__":
    main()
