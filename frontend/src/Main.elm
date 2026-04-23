module Main exposing (main)

{-| Hortus — interface principale.

Modèle simple :
- on appelle le backend pour créer une simulation
- on affiche la grille du jardin (SVG), le catalogue, le pantry, les événements
- l'utilisateur peut semer (sélectionne une espèce, clique sur une cellule libre)
  et avancer le temps (boutons +1j, +7j, +30j, +365j)

-}

import Browser
import Html exposing (Html, button, dd, div, dl, dt, h1, h2, h3, p, span, text)
import Html.Attributes as A
import Html.Events as E
import Http
import Json.Decode as D exposing (Decoder)
import Json.Encode as Encode
import Svg
import Svg.Attributes as SA
import Svg.Events as SE


-- =============================================================================
-- MAIN
-- =============================================================================


main : Program Flags Model Msg
main =
    Browser.element
        { init = init
        , update = update
        , view = view
        , subscriptions = \_ -> Sub.none
        }


-- =============================================================================
-- MODEL
-- =============================================================================


type alias Flags =
    { backendUrl : String }


type alias Model =
    { backendUrl : String
    , status : Status
    , simId : Maybe String
    , state : Maybe SimSnapshot
    , catalog : List SpeciesCard
    , selectedSpecies : Maybe String
    , selectedCell : Maybe ( Int, Int )
    , error : Maybe String
    }


type Status
    = Idle
    | Loading


init : Flags -> ( Model, Cmd Msg )
init flags =
    ( { backendUrl = flags.backendUrl
      , status = Idle
      , simId = Nothing
      , state = Nothing
      , catalog = []
      , selectedSpecies = Nothing
      , selectedCell = Nothing
      , error = Nothing
      }
    , Cmd.none
    )


-- =============================================================================
-- DTOs
-- =============================================================================


type alias SimDate =
    { year : Int, dayOfYear : Int }


type alias Stats =
    { daysSimulated : Int
    , daysFullyCovered : Int
    , daysInDeficit : Int
    , totalHarvestG : Float
    , totalFoodLostG : Float
    }


type alias PlantSnap =
    { speciesId : String
    , speciesName : String
    , stage : String
    , progress : Float
    , health : Float
    , biomassG : Float
    , harvestCount : Int
    }


type alias CellSnap =
    { col : Int
    , row : Int
    , soilType : String
    , cover : String
    , n : Float
    , p : Float
    , k : Float
    , organicMatterPct : Float
    , ph : Float
    , waterMm : Float
    , soilTempC : Float
    , plant : Maybe PlantSnap
    }


type alias GardenSnap =
    { cols : Int, rows : Int, cellAreaM2 : Float, cells : List CellSnap }


type alias PantrySnap =
    { totalMassG : Float
    , bySpecies : List ( String, Float )
    , items : List PantryItem
    }


type alias PantryItem =
    { speciesId : String
    , speciesName : String
    , compartment : String
    , massG : Float
    , daysLeft : Int
    }


type alias HouseholdSnap =
    { adults : Int, children : Int, equivalentAdults : Float }


type alias DailyEvent =
    { year : Int, dayOfYear : Int, kind : String, message : String }


type alias DailyWeather =
    { kind : String
    , tempMinC : Float
    , tempMaxC : Float
    , precipitationMm : Float
    , photoperiodH : Float
    }


type alias DailyBalance =
    { coverageAvg : Float
    , fullyCovered : Bool
    , deficits : List String
    }


type alias SimSnapshot =
    { id : String
    , date : SimDate
    , stats : Stats
    , garden : GardenSnap
    , pantry : PantrySnap
    , household : HouseholdSnap
    , recentEvents : List DailyEvent
    , lastWeather : Maybe DailyWeather
    , lastBalance : Maybe DailyBalance
    }


type alias SpeciesCard =
    { id : String
    , nameFr : String
    , nameLatin : String
    , family : String
    , lifeCycle : String
    , kcalPer100g : Float
    , gPerPlantOptimal : Float
    , daysToMaturity : Int
    , nitrogenFixer : Bool
    }


-- =============================================================================
-- UPDATE
-- =============================================================================


type Msg
    = NewSim
    | GotNewSim (Result Http.Error SimSnapshot)
    | GotState (Result Http.Error SimSnapshot)
    | GotCatalog (Result Http.Error (List SpeciesCard))
    | SelectSpecies String
    | SelectCell Int Int
    | ClickSow Int Int
    | GotSow (Result Http.Error SimSnapshot)
    | Advance Int
    | GotAdvance (Result Http.Error SimSnapshot)
    | WaterCell Int Int Float
    | MulchCell Int Int
    | CompostCell Int Int Float
    | UprootCell Int Int
    | TransformItem String String String Float
    | GotAction (Result Http.Error SimSnapshot)


update : Msg -> Model -> ( Model, Cmd Msg )
update msg model =
    case msg of
        NewSim ->
            ( { model | status = Loading, error = Nothing }
            , createSim model.backendUrl
            )

        GotNewSim (Ok snap) ->
            ( { model
                | status = Idle
                , simId = Just snap.id
                , state = Just snap
                , error = Nothing
              }
            , fetchCatalog model.backendUrl snap.id
            )

        GotNewSim (Err e) ->
            ( { model | status = Idle, error = Just (httpErr e) }, Cmd.none )

        GotState (Ok snap) ->
            ( { model | status = Idle, state = Just snap, error = Nothing }, Cmd.none )

        GotState (Err e) ->
            ( { model | status = Idle, error = Just (httpErr e) }, Cmd.none )

        GotCatalog (Ok cs) ->
            ( { model | catalog = cs }, Cmd.none )

        GotCatalog (Err e) ->
            ( { model | error = Just (httpErr e) }, Cmd.none )

        SelectSpecies sid ->
            ( { model
                | selectedSpecies =
                    if model.selectedSpecies == Just sid then
                        Nothing

                    else
                        Just sid
              }
            , Cmd.none
            )

        SelectCell col row ->
            ( { model | selectedCell = Just ( col, row ) }, Cmd.none )

        ClickSow col row ->
            case ( model.simId, model.selectedSpecies ) of
                ( Just sid, Just speciesId ) ->
                    ( { model | status = Loading }
                    , sowAt model.backendUrl sid col row speciesId
                    )

                _ ->
                    ( { model | error = Just "Sélectionne d'abord une espèce." }, Cmd.none )

        GotSow (Ok snap) ->
            ( { model | status = Idle, state = Just snap, error = Nothing }, Cmd.none )

        GotSow (Err e) ->
            ( { model | status = Idle, error = Just (httpErr e) }, Cmd.none )

        Advance days ->
            case model.simId of
                Just sid ->
                    ( { model | status = Loading }, advance model.backendUrl sid days )

                Nothing ->
                    ( model, Cmd.none )

        GotAdvance (Ok snap) ->
            ( { model | status = Idle, state = Just snap, error = Nothing }, Cmd.none )

        GotAdvance (Err e) ->
            ( { model | status = Idle, error = Just (httpErr e) }, Cmd.none )

        WaterCell col row mm ->
            withSim model (\sid -> waterCell model.backendUrl sid col row mm)

        MulchCell col row ->
            withSim model (\sid -> mulchCell model.backendUrl sid col row)

        CompostCell col row kg ->
            withSim model (\sid -> compostCell model.backendUrl sid col row kg)

        UprootCell col row ->
            withSim model (\sid -> uprootCell model.backendUrl sid col row)

        TransformItem speciesId from to mass ->
            withSim model (\sid -> transformItem model.backendUrl sid speciesId from to mass)

        GotAction (Ok snap) ->
            ( { model | status = Idle, state = Just snap, error = Nothing }, Cmd.none )

        GotAction (Err e) ->
            ( { model | status = Idle, error = Just (httpErr e) }, Cmd.none )


withSim : Model -> (String -> Cmd Msg) -> ( Model, Cmd Msg )
withSim model toCmd =
    case model.simId of
        Just sid ->
            ( { model | status = Loading, error = Nothing }, toCmd sid )

        Nothing ->
            ( { model | error = Just "Aucune simulation active." }, Cmd.none )


httpErr : Http.Error -> String
httpErr e =
    case e of
        Http.BadUrl s -> "URL invalide : " ++ s
        Http.Timeout -> "Timeout"
        Http.NetworkError -> "Réseau injoignable — backend démarré ?"
        Http.BadStatus code -> "HTTP " ++ String.fromInt code
        Http.BadBody msg -> "Réponse JSON invalide : " ++ msg


-- =============================================================================
-- HTTP
-- =============================================================================


createSim : String -> Cmd Msg
createSim url =
    Http.post
        { url = url ++ "/sim/new"
        , body =
            Http.jsonBody
                (Encode.object
                    [ ( "seed", Encode.int 42 )
                    , ( "plant_pilot_plan", Encode.bool True )
                    ]
                )
        , expect = Http.expectJson GotNewSim simSnapshotDecoder
        }


fetchCatalog : String -> String -> Cmd Msg
fetchCatalog url simId =
    Http.get
        { url = url ++ "/sim/" ++ simId ++ "/catalog"
        , expect = Http.expectJson GotCatalog (D.list speciesCardDecoder)
        }


sowAt : String -> String -> Int -> Int -> String -> Cmd Msg
sowAt url simId col row speciesId =
    Http.post
        { url = url ++ "/sim/" ++ simId ++ "/sow"
        , body =
            Http.jsonBody
                (Encode.object
                    [ ( "col", Encode.int col )
                    , ( "row", Encode.int row )
                    , ( "species_id", Encode.string speciesId )
                    ]
                )
        , expect = Http.expectJson GotSow (D.field "state" simSnapshotDecoder)
        }


advance : String -> String -> Int -> Cmd Msg
advance url simId days =
    Http.post
        { url = url ++ "/sim/" ++ simId ++ "/advance"
        , body = Http.jsonBody (Encode.object [ ( "days", Encode.int days ) ])
        , expect = Http.expectJson GotAdvance (D.field "state" simSnapshotDecoder)
        }


waterCell : String -> String -> Int -> Int -> Float -> Cmd Msg
waterCell url simId col row mm =
    Http.post
        { url = url ++ "/sim/" ++ simId ++ "/water"
        , body =
            Http.jsonBody
                (Encode.object
                    [ ( "col", Encode.int col )
                    , ( "row", Encode.int row )
                    , ( "mm", Encode.float mm )
                    ]
                )
        , expect = Http.expectJson GotAction (D.field "state" simSnapshotDecoder)
        }


mulchCell : String -> String -> Int -> Int -> Cmd Msg
mulchCell url simId col row =
    Http.post
        { url = url ++ "/sim/" ++ simId ++ "/mulch"
        , body =
            Http.jsonBody
                (Encode.object
                    [ ( "col", Encode.int col )
                    , ( "row", Encode.int row )
                    ]
                )
        , expect = Http.expectJson GotAction (D.field "state" simSnapshotDecoder)
        }


compostCell : String -> String -> Int -> Int -> Float -> Cmd Msg
compostCell url simId col row kg =
    Http.post
        { url = url ++ "/sim/" ++ simId ++ "/compost"
        , body =
            Http.jsonBody
                (Encode.object
                    [ ( "col", Encode.int col )
                    , ( "row", Encode.int row )
                    , ( "kg_per_m2", Encode.float kg )
                    ]
                )
        , expect = Http.expectJson GotAction (D.field "state" simSnapshotDecoder)
        }


uprootCell : String -> String -> Int -> Int -> Cmd Msg
uprootCell url simId col row =
    Http.post
        { url = url ++ "/sim/" ++ simId ++ "/uproot"
        , body =
            Http.jsonBody
                (Encode.object
                    [ ( "col", Encode.int col )
                    , ( "row", Encode.int row )
                    ]
                )
        , expect = Http.expectJson GotAction (D.field "state" simSnapshotDecoder)
        }


transformItem : String -> String -> String -> String -> String -> Float -> Cmd Msg
transformItem url simId speciesId from to mass =
    Http.post
        { url = url ++ "/sim/" ++ simId ++ "/transform"
        , body =
            Http.jsonBody
                (Encode.object
                    [ ( "species_id", Encode.string speciesId )
                    , ( "from", Encode.string from )
                    , ( "to", Encode.string to )
                    , ( "mass_g", Encode.float mass )
                    ]
                )
        , expect = Http.expectJson GotAction (D.field "state" simSnapshotDecoder)
        }


-- =============================================================================
-- DECODERS
-- =============================================================================


andMap : Decoder a -> Decoder (a -> b) -> Decoder b
andMap =
    D.map2 (|>)


simDateDecoder : Decoder SimDate
simDateDecoder =
    D.map2 SimDate
        (D.field "year" D.int)
        (D.field "day_of_year" D.int)


statsDecoder : Decoder Stats
statsDecoder =
    D.map5 Stats
        (D.field "days_simulated" D.int)
        (D.field "days_fully_covered" D.int)
        (D.field "days_in_deficit" D.int)
        (D.field "total_harvest_g" D.float)
        (D.field "total_food_lost_g" D.float)


plantSnapDecoder : Decoder PlantSnap
plantSnapDecoder =
    D.succeed PlantSnap
        |> andMap (D.field "species_id" D.string)
        |> andMap (D.field "species_name" D.string)
        |> andMap (D.field "stage" stageStringDecoder)
        |> andMap (D.field "progress" D.float)
        |> andMap (D.field "health" D.float)
        |> andMap (D.field "biomass_g" D.float)
        |> andMap (D.field "harvest_count" D.int)


stageStringDecoder : Decoder String
stageStringDecoder =
    D.oneOf
        [ D.string
        , D.succeed "?"
        ]


cellSnapDecoder : Decoder CellSnap
cellSnapDecoder =
    D.succeed CellSnap
        |> andMap (D.field "col" D.int)
        |> andMap (D.field "row" D.int)
        |> andMap (D.field "soil_type" D.string)
        |> andMap (D.field "cover" D.string)
        |> andMap (D.field "n" D.float)
        |> andMap (D.field "p" D.float)
        |> andMap (D.field "k" D.float)
        |> andMap (D.field "organic_matter_pct" D.float)
        |> andMap (D.field "ph" D.float)
        |> andMap (D.field "water_mm" D.float)
        |> andMap (D.field "soil_temp_c" D.float)
        |> andMap (D.field "plant" (D.nullable plantSnapDecoder))


gardenSnapDecoder : Decoder GardenSnap
gardenSnapDecoder =
    D.map4 GardenSnap
        (D.field "cols" D.int)
        (D.field "rows" D.int)
        (D.field "cell_area_m2" D.float)
        (D.field "cells" (D.list cellSnapDecoder))


pantrySnapDecoder : Decoder PantrySnap
pantrySnapDecoder =
    D.map3 PantrySnap
        (D.field "total_mass_g" D.float)
        (D.field "by_species" (D.list (tupleDecoder D.string D.float)))
        (D.field "items" (D.list pantryItemDecoder))


pantryItemDecoder : Decoder PantryItem
pantryItemDecoder =
    D.map5 PantryItem
        (D.field "species_id" D.string)
        (D.field "species_name" D.string)
        (D.field "compartment" D.string)
        (D.field "mass_g" D.float)
        (D.field "days_left" D.int)


householdSnapDecoder : Decoder HouseholdSnap
householdSnapDecoder =
    D.map3 HouseholdSnap
        (D.field "adults" D.int)
        (D.field "children" D.int)
        (D.field "equivalent_adults" D.float)


tupleDecoder : Decoder a -> Decoder b -> Decoder ( a, b )
tupleDecoder a b =
    D.map2 Tuple.pair (D.index 0 a) (D.index 1 b)


eventDecoder : Decoder DailyEvent
eventDecoder =
    D.map4 DailyEvent
        (D.at [ "date", "year" ] D.int)
        (D.at [ "date", "day_of_year" ] D.int)
        (D.field "kind" D.string)
        (D.field "message" D.string)


weatherDecoder : Decoder DailyWeather
weatherDecoder =
    D.map5 DailyWeather
        (D.field "kind" D.string)
        (D.field "temp_min_c" D.float)
        (D.field "temp_max_c" D.float)
        (D.field "precipitation_mm" D.float)
        (D.field "photoperiod_h" D.float)


balanceDecoder : Decoder DailyBalance
balanceDecoder =
    D.map3 DailyBalance
        (D.field "coverage_avg" D.float)
        (D.field "fully_covered" D.bool)
        (D.field "deficits" (D.list D.string))


simSnapshotDecoder : Decoder SimSnapshot
simSnapshotDecoder =
    D.succeed SimSnapshot
        |> andMap (D.field "id" D.string)
        |> andMap (D.field "date" simDateDecoder)
        |> andMap (D.field "stats" statsDecoder)
        |> andMap (D.field "garden" gardenSnapDecoder)
        |> andMap (D.field "pantry" pantrySnapDecoder)
        |> andMap (D.field "household" householdSnapDecoder)
        |> andMap (D.field "recent_events" (D.list eventDecoder))
        |> andMap (D.field "last_weather" (D.nullable weatherDecoder))
        |> andMap (D.field "last_balance" (D.nullable balanceDecoder))


speciesCardDecoder : Decoder SpeciesCard
speciesCardDecoder =
    D.succeed SpeciesCard
        |> andMap (D.field "id" D.string)
        |> andMap (D.field "name_fr" D.string)
        |> andMap (D.field "name_latin" D.string)
        |> andMap (D.field "family" D.string)
        |> andMap (D.field "life_cycle" D.string)
        |> andMap (D.field "kcal_per_100g" D.float)
        |> andMap (D.field "g_per_plant_optimal" D.float)
        |> andMap (D.field "days_to_maturity" D.int)
        |> andMap (D.field "nitrogen_fixer" D.bool)


-- =============================================================================
-- VIEW
-- =============================================================================


view : Model -> Html Msg
view model =
    div [ A.class "app" ]
        [ viewHeader model
        , case model.error of
            Just e -> div [ A.class "error" ] [ text e ]
            Nothing -> text ""
        , case model.state of
            Nothing -> viewWelcome model
            Just snap -> viewMain model snap
        ]


viewHeader : Model -> Html Msg
viewHeader model =
    div [ A.class "header" ]
        [ h1 [] [ text "🌱 Hortus" ]
        , div [ A.class "meta" ]
            (case model.state of
                Nothing -> [ span [] [ text "déconnecté" ] ]
                Just s ->
                    [ span [] [ text "📅 ", strongTxt ("an " ++ String.fromInt s.date.year ++ " · jour " ++ String.fromInt s.date.dayOfYear) ]
                    , span [] [ text "🏠 ", strongTxt (String.fromInt s.household.adults ++ " adulte(s)") ]
                    , span [] [ text "🌾 ", strongTxt (autonomyPct s.stats ++ "% autonomie") ]
                    , span [] [ text "📦 ", strongTxt (formatKg s.pantry.totalMassG ++ " stock") ]
                    ]
            )
        ]


viewWelcome : Model -> Html Msg
viewWelcome model =
    div [ A.class "panel" ]
        [ h2 [] [ text "Démarrer une simulation" ]
        , p [] [ text "Crée un jardin de 25 m² à Paris, avec un pommier établi en place. Tu pourras semer les autres cultures et avancer le temps." ]
        , div [ A.class "controls" ]
            [ button
                [ A.class "primary"
                , E.onClick NewSim
                , A.disabled (model.status == Loading)
                ]
                [ text (if model.status == Loading then "Création..." else "🌱 Nouveau jardin") ]
            ]
        ]


viewMain : Model -> SimSnapshot -> Html Msg
viewMain model snap =
    div [ A.class "layout" ]
        [ div []
            [ viewControls model snap
            , viewWeather snap
            , viewGarden model snap
            , viewSelectedCell model snap
            ]
        , div []
            [ viewCatalog model
            , viewStats snap
            , viewPantry snap
            , viewKitchen model snap
            , viewEvents snap
            ]
        ]


viewControls : Model -> SimSnapshot -> Html Msg
viewControls model _ =
    div [ A.class "panel" ]
        [ h2 [] [ text "Avancer le temps" ]
        , div [ A.class "controls" ]
            [ btnAdvance model 1 "+1 j"
            , btnAdvance model 7 "+1 sem"
            , btnAdvance model 30 "+1 mois"
            , btnAdvance model 90 "+3 mois"
            , btnAdvance model 365 "+1 an"
            , button
                [ A.class "danger"
                , E.onClick NewSim
                , A.disabled (model.status == Loading)
                ]
                [ text "Réinitialiser" ]
            ]
        ]


btnAdvance : Model -> Int -> String -> Html Msg
btnAdvance model days label =
    button
        [ E.onClick (Advance days)
        , A.disabled (model.status == Loading)
        ]
        [ text label ]


viewWeather : SimSnapshot -> Html Msg
viewWeather snap =
    case snap.lastWeather of
        Nothing -> text ""
        Just w ->
            div [ A.class "panel" ]
                [ h2 [] [ text "Météo du jour" ]
                , div [ A.class "weather-row" ]
                    [ span [ A.class "weather-icon" ] [ text (weatherIcon w.kind) ]
                    , span [] [ text (w.kind ++ " — " ++ String.fromFloat (toOneDecimal w.tempMinC) ++ "° / " ++ String.fromFloat (toOneDecimal w.tempMaxC) ++ "°") ]
                    , span [] [ text ("☔ " ++ String.fromFloat (toOneDecimal w.precipitationMm) ++ " mm") ]
                    , span [] [ text ("☀ " ++ String.fromFloat (toOneDecimal w.photoperiodH) ++ " h") ]
                    ]
                ]


weatherIcon : String -> String
weatherIcon kind =
    case kind of
        "Clear" -> "☀️"
        "PartlyCloudy" -> "⛅"
        "Overcast" -> "☁️"
        "LightRain" -> "🌦️"
        "HeavyRain" -> "🌧️"
        "Storm" -> "⛈️"
        "Snow" -> "❄️"
        "Frost" -> "🧊"
        "Heatwave" -> "🥵"
        "Fog" -> "🌫️"
        _ -> "🌡️"


viewGarden : Model -> SimSnapshot -> Html Msg
viewGarden model snap =
    let
        g = snap.garden
        size = 36
        gap = 2
        w = g.cols * (size + gap)
        h = g.rows * (size + gap)
    in
    div [ A.class "panel" ]
        [ h2 [] [ text ("Jardin " ++ String.fromFloat (toOneDecimal (toFloat g.cols * sqrt g.cellAreaM2)) ++ " m × " ++ String.fromFloat (toOneDecimal (toFloat g.rows * sqrt g.cellAreaM2)) ++ " m") ]
        , Svg.svg
            [ SA.viewBox ("0 0 " ++ String.fromInt w ++ " " ++ String.fromInt h)
            , SA.width (String.fromInt w)
            , SA.height (String.fromInt h)
            , SA.style "max-width:100%;height:auto"
            ]
            (List.map (viewCell model size gap) g.cells)
        ]


viewCell : Model -> Int -> Int -> CellSnap -> Svg.Svg Msg
viewCell model size gap cell =
    let
        x = cell.col * (size + gap)
        y = cell.row * (size + gap)
        isSelected = model.selectedCell == Just ( cell.col, cell.row )
        ( fill, stroke ) = cellColors cell
        clickHandler =
            if cell.plant == Nothing && model.selectedSpecies /= Nothing then
                ClickSow cell.col cell.row
            else
                SelectCell cell.col cell.row
    in
    Svg.g [ SE.onClick clickHandler, SA.style "cursor:pointer" ]
        ([ Svg.rect
            [ SA.x (String.fromInt x)
            , SA.y (String.fromInt y)
            , SA.width (String.fromInt size)
            , SA.height (String.fromInt size)
            , SA.fill fill
            , SA.stroke
                (if isSelected then "#d4b85a" else stroke)
            , SA.strokeWidth
                (if isSelected then "3" else "1")
            , SA.rx "2"
            ]
            []
         ]
            ++ plantGlyph x y size cell
        )


cellColors : CellSnap -> ( String, String )
cellColors cell =
    case cell.plant of
        Just p ->
            let
                progress = clamp 0 1 p.progress
                health = clamp 0 1 p.health
                -- vert qui s'éclaircit avec la maturité, atténué par la santé
                base =
                    if String.contains "tree" p.speciesId || String.contains "apple" p.speciesId then
                        "#5a7a35"
                    else if String.contains "bean" p.speciesId then
                        "#9bc55c"
                    else if String.contains "tomato" p.speciesId then
                        "#c66339"
                    else if String.contains "kale" p.speciesId then
                        "#3a5d2b"
                    else if String.contains "carrot" p.speciesId then
                        "#e08433"
                    else
                        "#6b9c47"
                _ = progress
                _ = health
            in
            ( base, "#3d2818" )

        Nothing ->
            -- Couleur sol selon humidité
            let
                fc = 60.0
                ratio = clamp 0 1 (cell.waterMm / fc)
            in
            if ratio > 0.7 then
                ( "#6b4f2f", "#3d2818" )
            else if ratio > 0.4 then
                ( "#8b6e3d", "#3d2818" )
            else
                ( "#b5946b", "#3d2818" )


plantGlyph : Int -> Int -> Int -> CellSnap -> List (Svg.Svg Msg)
plantGlyph x y size cell =
    case cell.plant of
        Nothing -> []
        Just p ->
            let
                cx = String.fromInt (x + size // 2)
                cy = String.fromInt (y + size // 2)
                glyph =
                    if String.contains "apple" p.speciesId then "🍎"
                    else if String.contains "tomato" p.speciesId then "🍅"
                    else if String.contains "carrot" p.speciesId then "🥕"
                    else if String.contains "bean" p.speciesId then "🌱"
                    else if String.contains "kale" p.speciesId then "🥬"
                    else "🌿"
            in
            [ Svg.text_
                [ SA.x cx
                , SA.y cy
                , SA.fontSize (String.fromInt (size - 12))
                , SA.textAnchor "middle"
                , SA.dominantBaseline "central"
                , SA.opacity (String.fromFloat (0.4 + 0.6 * p.progress))
                ]
                [ Svg.text glyph ]
            ]


viewSelectedCell : Model -> SimSnapshot -> Html Msg
viewSelectedCell model snap =
    case model.selectedCell of
        Nothing -> text ""
        Just ( col, row ) ->
            let
                cellMaybe =
                    snap.garden.cells
                        |> List.filter (\c -> c.col == col && c.row == row)
                        |> List.head
            in
            case cellMaybe of
                Nothing -> text ""
                Just c ->
                    div [ A.class "panel" ]
                        [ h3 [] [ text ("Cellule (" ++ String.fromInt col ++ ", " ++ String.fromInt row ++ ")") ]
                        , div [ A.class "cell-info" ]
                            [ dl []
                                [ dt [] [ text "Sol" ], dd [] [ text (c.soilType ++ " · " ++ coverLabel c.cover) ]
                                , dt [] [ text "Eau" ], dd [] [ text (String.fromFloat (toOneDecimal c.waterMm) ++ " mm") ]
                                , dt [] [ text "T° sol" ], dd [] [ text (String.fromFloat (toOneDecimal c.soilTempC) ++ " °C") ]
                                , dt [] [ text "N / P / K" ], dd [] [ text (npk c) ]
                                , dt [] [ text "MO" ], dd [] [ text (String.fromFloat (toOneDecimal c.organicMatterPct) ++ " %") ]
                                , dt [] [ text "pH" ], dd [] [ text (String.fromFloat (toOneDecimal c.ph)) ]
                                ]
                            , case c.plant of
                                Nothing -> p [] [ text "(libre)" ]
                                Just p_ ->
                                    p []
                                        [ strongTxt p_.speciesName
                                        , text (" — " ++ p_.stage ++ " · biomasse " ++ String.fromFloat (toZeroDecimal p_.biomassG) ++ " g · santé " ++ String.fromFloat (toZeroDecimal (p_.health * 100)) ++ "%")
                                        ]
                            ]
                        , h3 [ A.style "margin-top" "0.8rem" ] [ text "Actions" ]
                        , div [ A.class "controls" ]
                            [ btnAction model (WaterCell col row 10) "💧 +10 mm"
                            , btnAction model (WaterCell col row 25) "💧💧 +25 mm"
                            , btnAction model (MulchCell col row) (if c.cover == "mulch" then "🍂 (paillé)" else "🍂 Pailler")
                            , btnAction model (CompostCell col row 1.0) "🌱 Compost 1 kg/m²"
                            , case c.plant of
                                Just _ -> btnAction model (UprootCell col row) "✂ Arracher"
                                Nothing -> text ""
                            ]
                        ]


btnAction : Model -> Msg -> String -> Html Msg
btnAction model msg label =
    button [ E.onClick msg, A.disabled (model.status == Loading) ] [ text label ]


coverLabel : String -> String
coverLabel c =
    case c of
        "bare" -> "nu"
        "mulch" -> "paillé"
        "living" -> "engrais vert"
        "crop" -> "cultivé"
        _ -> c


viewCatalog : Model -> Html Msg
viewCatalog model =
    div [ A.class "panel" ]
        [ h2 [] [ text "Catalogue d'espèces" ]
        , p [ A.style "font-size" "0.8rem", A.style "color" "#5a3a22" ]
            [ text "Sélectionne une espèce puis clique sur une cellule libre pour semer." ]
        , div [ A.class "species-grid" ]
            (List.map (viewSpeciesRow model) model.catalog)
        ]


viewSpeciesRow : Model -> SpeciesCard -> Html Msg
viewSpeciesRow model sp =
    let
        isSelected = model.selectedSpecies == Just sp.id
    in
    div
        [ A.classList [ ( "species-row", True ), ( "selected", isSelected ) ]
        , E.onClick (SelectSpecies sp.id)
        ]
        [ span []
            [ text sp.nameFr
            , span [ A.class "latin" ] [ text sp.nameLatin ]
            ]
        , span [ A.style "font-size" "0.75rem", A.style "color" "#5a3a22" ]
            [ text (String.fromInt sp.daysToMaturity ++ "j · " ++ String.fromFloat (toZeroDecimal sp.kcalPer100g) ++ " kcal") ]
        ]


viewStats : SimSnapshot -> Html Msg
viewStats snap =
    div [ A.class "panel" ]
        [ h2 [] [ text "Bilan" ]
        , div [ A.class "stats-grid" ]
            [ span [ A.class "label" ] [ text "Jours simulés" ]
            , span [ A.class "value" ] [ text (String.fromInt snap.stats.daysSimulated) ]
            , span [ A.class "label" ] [ text "Jours autonomie" ]
            , span [ A.class "value" ] [ text (String.fromInt snap.stats.daysFullyCovered) ]
            , span [ A.class "label" ] [ text "Jours déficit" ]
            , span [ A.class "value" ] [ text (String.fromInt snap.stats.daysInDeficit) ]
            , span [ A.class "label" ] [ text "Récolte totale" ]
            , span [ A.class "value" ] [ text (formatKg snap.stats.totalHarvestG) ]
            , span [ A.class "label" ] [ text "Pertes pantry" ]
            , span [ A.class "value" ] [ text (formatKg snap.stats.totalFoodLostG) ]
            ]
        , if snap.stats.daysSimulated > 0 then
            div [ A.class "day-bar" ]
                [ div
                    [ A.class "ok"
                    , A.style "width" (String.fromFloat (100 * toFloat snap.stats.daysFullyCovered / toFloat snap.stats.daysSimulated) ++ "%")
                    ]
                    []
                , div
                    [ A.class "deficit"
                    , A.style "width" (String.fromFloat (100 * toFloat snap.stats.daysInDeficit / toFloat snap.stats.daysSimulated) ++ "%")
                    ]
                    []
                ]

          else
            text ""
        ]


viewPantry : SimSnapshot -> Html Msg
viewPantry snap =
    div [ A.class "panel" ]
        [ h2 [] [ text "Garde-manger" ]
        , if List.isEmpty snap.pantry.bySpecies then
            p [ A.style "color" "#8b6e3d", A.style "font-size" "0.85rem" ]
                [ text "Vide. Avance le temps après avoir semé." ]

          else
            div [ A.class "pantry-list" ]
                (List.map
                    (\( name, g ) ->
                        div [ A.class "pantry-row" ]
                            [ span [] [ text name ]
                            , span [] [ text (String.fromFloat (toZeroDecimal g) ++ " g") ]
                            ]
                    )
                    snap.pantry.bySpecies
                )
        ]


viewKitchen : Model -> SimSnapshot -> Html Msg
viewKitchen model snap =
    let
        freshItems =
            snap.pantry.items
                |> List.filter (\it -> it.compartment == "frais" && it.massG > 5)
    in
    if List.isEmpty freshItems then
        text ""

    else
        div [ A.class "panel" ]
            [ h2 [] [ text "🥘 Cuisine" ]
            , p [ A.style "font-size" "0.78rem", A.style "color" "#5a3a22" ]
                [ text "Transforme le frais (durée courte) en stockage longue durée." ]
            , div [ A.class "pantry-list" ]
                (List.map (viewKitchenRow model) freshItems)
            ]


viewKitchenRow : Model -> PantryItem -> Html Msg
viewKitchenRow model it =
    let
        amount =
            min 200.0 it.massG

        btn target label =
            button
                [ E.onClick (TransformItem it.speciesId "fresh" target amount)
                , A.disabled (model.status == Loading)
                , A.style "padding" "3px 6px"
                , A.style "font-size" "0.75rem"
                , A.style "margin" "1px"
                ]
                [ text label ]
    in
    div
        [ A.class "pantry-row"
        , A.style "flex-direction" "column"
        , A.style "align-items" "stretch"
        , A.style "padding" "0.4rem 0"
        ]
        [ div [ A.style "display" "flex", A.style "justify-content" "space-between", A.style "margin-bottom" "0.2rem" ]
            [ span [] [ strongTxt it.speciesName, text (" · " ++ String.fromFloat (toZeroDecimal it.massG) ++ " g") ]
            , span [ A.style "font-size" "0.75rem", A.style "color" "#5a3a22" ]
                [ text ("expire dans " ++ String.fromInt it.daysLeft ++ "j") ]
            ]
        , div [ A.style "display" "flex", A.style "flex-wrap" "wrap" ]
            [ btn "cellar" "→ cellier"
            , btn "lacto" "→ lacto"
            , btn "canned" "→ conserve"
            , btn "frozen" "→ congel"
            , btn "dry" "→ sec"
            ]
        ]


viewEvents : SimSnapshot -> Html Msg
viewEvents snap =
    div [ A.class "panel" ]
        [ h2 [] [ text "Journal" ]
        , div [ A.class "events-list" ]
            (if List.isEmpty snap.recentEvents then
                [ p [ A.style "color" "#8b6e3d", A.style "font-size" "0.8rem" ] [ text "(aucun événement)" ] ]

             else
                List.map viewEvent snap.recentEvents
            )
        ]


viewEvent : DailyEvent -> Html Msg
viewEvent ev =
    div [ A.classList [ ( "event", True ), ( eventClass ev.kind, True ) ] ]
        [ span [ A.class "date" ]
            [ text ("y" ++ String.fromInt ev.year ++ "/" ++ String.padLeft 3 '0' (String.fromInt ev.dayOfYear)) ]
        , text ev.message
        ]


eventClass : String -> String
eventClass kind =
    case kind of
        "Harvested" -> "harvested"
        "Deficit" -> "deficit"
        "FrostKilled" -> "frost"
        _ -> ""


-- =============================================================================
-- UTILS
-- =============================================================================


strongTxt : String -> Html msg
strongTxt s =
    Html.strong [] [ text s ]


toOneDecimal : Float -> Float
toOneDecimal x =
    (toFloat (round (x * 10))) / 10


toZeroDecimal : Float -> Float
toZeroDecimal x =
    toFloat (round x)


formatKg : Float -> String
formatKg g =
    if g >= 1000 then
        String.fromFloat (toOneDecimal (g / 1000)) ++ " kg"

    else
        String.fromFloat (toZeroDecimal g) ++ " g"


autonomyPct : Stats -> String
autonomyPct s =
    if s.daysSimulated == 0 then
        "0"

    else
        String.fromFloat (toZeroDecimal (100 * toFloat s.daysFullyCovered / toFloat s.daysSimulated))


npk : CellSnap -> String
npk c =
    String.join " / "
        [ String.fromFloat (toOneDecimal c.n)
        , String.fromFloat (toOneDecimal c.p)
        , String.fromFloat (toOneDecimal c.k)
        ]
