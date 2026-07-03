module Main exposing (main)

{-| Hortus — calendrier des plantations + carnet de jardin.

Deux vues :
- 📅 Calendrier : fenêtres de semis/récolte par espèce, ajustées au climat local.
- 📓 Mon jardin : parcelles + journal d'actions datées (CRUD contre SQLite).

-}

import Browser
import Browser.Events
import Html exposing (Html, button, div, h1, h2, h3, input, option, p, select, span, text, textarea)
import Html.Attributes as A
import Html.Events as E
import Http
import Json.Decode as D exposing (Decoder)
import Json.Encode as Encode
import Svg
import Svg.Attributes as SA
import Svg.Events as SE


main : Program Flags Model Msg
main =
    Browser.element
        { init = init
        , update = update
        , view = view
        , subscriptions = subscriptions
        }


subscriptions : Model -> Sub Msg
subscriptions model =
    case ( model.dragging, model.panning ) of
        ( Just _, _ ) -> Browser.Events.onMouseUp (D.succeed DragEnd)
        ( _, Just _ ) -> Browser.Events.onMouseUp (D.succeed GardenPanEnd)
        _ -> Sub.none



-- MODEL


type alias Flags =
    { backendUrl : String, today : String }


type TileState
    = TileEmpty
    | TileTilled
    | TileSown String -- species_id
    | TileGrowing String Float -- species_id, progress 0..1
    | TileMature String
    | TileHarvested String


type ViewMode
    = CoachView
    | CalendarView
    | JournalView
    | AlmanacView


type alias Model =
    { backendUrl : String
    , viewMode : ViewMode
    , cities : List City
    , selectedCity : String
    , calendar : Maybe CalendarResponse
    , loading : Bool
    , refreshingClimate : Bool
    , error : Maybe String
    , filterCategory : Maybe String
    , filterDifficulty : Maybe String
    , search : String
    , selectedSpecies : Maybe String
    -- Journal
    , parcels : List Parcel
    , actions : List ActionEntry
    , actionKinds : List String
    , parcelForm : ParcelForm
    , editingParcel : Maybe Int
    , actionForm : ActionForm
    , editingAction : Maybe Int
    , filterActionParcel : Maybe Int
    , filterActionKind : Maybe String
    , today : String
    , forecast : List ForecastDay
    , historical : List HistoricalDay
    , historicalLoading : Bool
    , paletteSpecies : Maybe String
    , hoverPlant : Maybe Int -- action id
    , movingPlant : Maybe Int -- id action en cours de déplacement (click-move)
    , dragging : Maybe DragState
    , plantMenu : Maybe Int -- id du plant avec menu ouvert
    , noteDraft : String -- texte observation en cours de saisie
    , almanacSearch : String -- filtre texte de l'almanach
    , solutionDraft : Maybe ( Int, String ) -- (id note, texte solution en édition)
    , problems : List Problem
    , newProblem : Maybe NewProblemDraft
    , entryDraft : Maybe EntryDraft
    , closeDraft : Maybe ( Int, String ) -- (id fiche, conclusion en édition)
    , bulkKind : String
    , bulkSpeciesId : Maybe String
    , bulkZone : Maybe Zone
    , bulkOnlyMature : Bool
    , cursorOnTerrain : Maybe ( Int, Int )
    , confirmingClearAll : Bool
    , viewDoy : Int -- jour de l'année visualisé (1..365)
    , gardenView : GardenView
    , panning : Maybe PanState
    }


type alias GardenView =
    { zoom : Float, panX : Float, panY : Float }


type alias PanState =
    { startMouseX : Int, startMouseY : Int, startPanX : Float, startPanY : Float }


type Zone
    = Shelter
    | Terrain


type alias DragState =
    { id : Int
    , fromZone : Zone
    , currentX : Int
    , currentY : Int
    , currentZone : Zone
    , moved : Bool
    }


type alias ForecastDay =
    { date : String
    , tempMinC : Float
    , tempMaxC : Float
    , precipitationMm : Float
    , windKmh : Float
    , kind : String
    }


type alias HistoricalDay =
    { doy : Int
    , tempMinC : Float
    , tempMaxC : Float
    , precipitationMm : Float
    , samples : Int
    }


type alias City =
    { slug : String, name : String, latitude : Float, longitude : Float }


type alias CalendarWindow =
    { doyStart : Int, doyEnd : Int }


type alias Species =
    { id : String
    , nameFr : String
    , nameLatin : String
    , family : String
    , lifeCycle : String
    , category : String
    , difficulty : String
    , indoorSow : Maybe CalendarWindow
    , directSow : Maybe CalendarWindow
    , transplant : Maybe CalendarWindow
    , harvest : CalendarWindow
    , depthCm : Float
    , spacingCm : Int
    , daysToHarvest : Int
    , notes : List String
    , friends : List String
    , foes : List String
    }


type alias SpeciesLocal =
    { species : Species
    , shiftDays : Int
    , indoorSowLocal : Maybe CalendarWindow
    , directSowLocal : Maybe CalendarWindow
    , transplantLocal : Maybe CalendarWindow
    , harvestLocal : CalendarWindow
    }


type alias Location =
    { name : String, latitude : Float, longitude : Float, altitudeM : Float }


type alias CalendarResponse =
    { location : Location, climateSource : String, species : List SpeciesLocal }


type alias Parcel =
    { id : Int
    , name : String
    , surfaceM2 : Maybe Float
    , exposition : Maybe String
    , soilNotes : Maybe String
    , gridX : Int
    , gridY : Int
    , gridW : Int
    , gridH : Int
    , color : String
    }


type alias ParcelForm =
    { name : String
    , surface : String
    , exposition : String
    , soilNotes : String
    , gridX : String
    , gridY : String
    , gridW : String
    , gridH : String
    , color : String
    }


emptyParcelForm : ParcelForm
emptyParcelForm =
    { name = "", surface = "", exposition = "", soilNotes = ""
    , gridX = "0", gridY = "0", gridW = "2", gridH = "2", color = "#8fbc4a"
    }


-- Fiche problème : suivi quasi scientifique d'un souci au jardin.
-- Timeline d'entrées datées observation/traitement/résultat, puis conclusion.
type alias Problem =
    { id : Int
    , speciesId : Maybe String
    , actionId : Maybe Int
    , title : String
    , category : String
    , status : String -- "open" | "resolved"
    , conclusion : Maybe String
    , entries : List ProblemEntry
    }


type alias ProblemEntry =
    { id : Int
    , problemId : Int
    , date : String
    , kind : String -- "observation" | "traitement" | "resultat"
    , text : String
    }


type alias NewProblemDraft =
    { speciesId : String
    , actionId : Maybe Int
    , title : String
    , category : String
    , firstObs : String
    }


type alias EntryDraft =
    { problemId : Int
    , kind : String
    , text : String
    }


type alias ActionEntry =
    { id : Int
    , date : String
    , parcelId : Maybe Int
    , speciesId : Maybe String
    , kind : String
    , quantityG : Maybe Float
    , notes : Maybe String
    , gridX : Maybe Int
    , gridY : Maybe Int
    , solution : Maybe String
    }


type alias ActionForm =
    { date : String
    , parcelId : String
    , speciesId : String
    , kind : String
    , quantity : String
    , notes : String
    , gridX : String
    , gridY : String
    }


emptyActionForm : String -> ActionForm
emptyActionForm today =
    { date = today
    , parcelId = ""
    , speciesId = ""
    , kind = "semis_direct"
    , quantity = ""
    , notes = ""
    , gridX = ""
    , gridY = ""
    }


init : Flags -> ( Model, Cmd Msg )
init flags =
    ( { backendUrl = flags.backendUrl
      , today = flags.today
      , viewMode = CoachView
      , cities = []
      , selectedCity = "le_bois_doingt"
      , calendar = Nothing
      , loading = True
      , refreshingClimate = False
      , error = Nothing
      , filterCategory = Nothing
      , filterDifficulty = Nothing
      , search = ""
      , selectedSpecies = Nothing
      , parcels = []
      , actions = []
      , actionKinds = []
      , parcelForm = emptyParcelForm
      , editingParcel = Nothing
      , actionForm = emptyActionForm flags.today
      , editingAction = Nothing
      , filterActionParcel = Nothing
      , filterActionKind = Nothing
      , forecast = []
      , historical = []
      , historicalLoading = False
      , paletteSpecies = Nothing
      , hoverPlant = Nothing
      , movingPlant = Nothing
      , dragging = Nothing
      , plantMenu = Nothing
      , noteDraft = ""
      , almanacSearch = ""
      , solutionDraft = Nothing
      , problems = []
      , newProblem = Nothing
      , entryDraft = Nothing
      , closeDraft = Nothing
      , bulkKind = "arrosage"
      , bulkSpeciesId = Nothing
      , bulkZone = Nothing
      , bulkOnlyMature = False
      , cursorOnTerrain = Nothing
      , confirmingClearAll = False
      , viewDoy = isoToDoy flags.today
      , gardenView = { zoom = 1, panX = 0, panY = 0 }
      , panning = Nothing
      }
    , Cmd.batch
        [ fetchCities flags.backendUrl
        , fetchCalendar flags.backendUrl "le_bois_doingt" False
        , fetchParcels flags.backendUrl
        , fetchActions flags.backendUrl
        , fetchActionKinds flags.backendUrl
        , fetchForecast flags.backendUrl "le_bois_doingt"
        , fetchProblems flags.backendUrl
        ]
    )



-- UPDATE


type Msg
    = NoOp
    | GotCities (Result Http.Error (List City))
    | GotCalendar (Result Http.Error CalendarResponse)
    | SetCity String
    | RefreshClimate
    | SetFilterCategory String
    | SetFilterDifficulty String
    | SetSearch String
    | SelectSpeciesRow String
    | SetViewMode ViewMode
      -- Journal
    | GotParcels (Result Http.Error (List Parcel))
    | GotActions (Result Http.Error (List ActionEntry))
    | GotActionKinds (Result Http.Error (List String))
    | GotParcelSaved (Result Http.Error Parcel)
    | GotActionSaved (Result Http.Error ActionEntry)
    | GotDeleted (Result Http.Error ())
    | GotBulkSaved (Result Http.Error ())
    | SetParcelName String
    | SetParcelSurface String
    | SetParcelExposition String
    | SetParcelSoilNotes String
    | SetParcelGridX String
    | SetParcelGridY String
    | SetParcelGridW String
    | SetParcelGridH String
    | SetParcelColor String
    | SubmitParcel
    | EditParcel Parcel
    | CancelEditParcel
    | DeleteParcel Int
    | SetActionDate String
    | SetActionParcel String
    | SetActionSpecies String
    | SetActionKind String
    | SetActionQty String
    | SetActionNotes String
    | SubmitAction
    | EditAction ActionEntry
    | CancelEditAction
    | DeleteAction Int
    | SetFilterActionParcel String
    | SetFilterActionKind String
    | GotForecast (Result Http.Error (List ForecastDay))
    | LoadHistorical
    | GotHistorical (Result Http.Error (List HistoricalDay))
    | SelectTile Int Int
    | SelectPaletteSpecies String
    | ClearPalette
    | PlaceAtPixel Int Int
    | HoverPlant (Maybe Int)
    | DeletePlant Int
    | PlaceInShelter Int Int
    | StartMoving Int
    | CancelMoving
    | DragStart Int Zone Int Int
    | DragMoveIn Zone Int Int
    | DragEnterZone Zone
    | DragEnd
    | OpenPlantMenu Int
    | ClosePlantMenu
    | QuickAction Int String -- plant id, kind
    | SetNoteDraft String
    | SaveObservation Int -- plant id
    | SetAlmanacSearch String
    | GotProblems (Result Http.Error (List Problem))
    | OpenProblemForm (Maybe String) (Maybe Int) -- espèce, plant préremplis
    | SetProblemSpecies String
    | SetProblemTitle String
    | SetProblemCategory String
    | SetProblemObs String
    | CancelProblemForm
    | SubmitProblem
    | GotProblemCreated (Result Http.Error Int)
    | StartEntry Int String -- id fiche, kind
    | SetEntryText String
    | CancelEntry
    | SubmitEntry
    | StartClose Int
    | SetCloseText String
    | CancelClose
    | SubmitClose
    | ReopenProblem Int
    | DeleteProblem Int
    | GotProblemSaved (Result Http.Error ())
    | EditSolution Int String -- id note, texte initial
    | SetSolutionDraft String
    | CancelSolution
    | SaveSolution Int -- id note
    | GotSolutionSaved (Result Http.Error ())
    | SetBulkKind String
    | SetBulkSpecies String
    | SetBulkZone String
    | ToggleBulkMature
    | ApplyBulk
    | TerrainCursorMove Int Int
    | TerrainCursorLeave
    | RequestClearAll
    | ConfirmClearAll
    | CancelClearAll
    | SetViewDoy Int
    | ResetViewDoy
    | GardenZoom Float Int Int -- deltaY, ox, oy
    | GardenZoomReset
    | GardenPanStart Bool Int Int -- altKey, x, y
    | GardenPanEnd


update : Msg -> Model -> ( Model, Cmd Msg )
update msg model =
    case msg of
        NoOp ->
            ( model, Cmd.none )

        GotCities (Ok cs) ->
            ( { model | cities = cs }, Cmd.none )

        GotCities (Err e) ->
            ( { model | error = Just (httpErr e) }, Cmd.none )

        GotCalendar (Ok c) ->
            ( { model | calendar = Just c, loading = False, refreshingClimate = False, error = Nothing }, Cmd.none )

        GotCalendar (Err e) ->
            ( { model | error = Just (httpErr e), loading = False, refreshingClimate = False }, Cmd.none )

        SetCity slug ->
            ( { model | selectedCity = slug, loading = True, calendar = Nothing }
            , Cmd.batch
                [ fetchCalendar model.backendUrl slug False
                , fetchForecast model.backendUrl slug
                ]
            )

        GotForecast (Ok f) -> ( { model | forecast = f }, Cmd.none )
        GotForecast (Err _) -> ( model, Cmd.none )

        LoadHistorical ->
            ( { model | historicalLoading = True }
            , fetchHistorical model.backendUrl model.selectedCity
            )

        GotHistorical (Ok h) ->
            ( { model | historical = h, historicalLoading = False }, Cmd.none )

        GotHistorical (Err e) ->
            ( { model | historicalLoading = False, error = Just (httpErr e) }, Cmd.none )

        SelectTile c r ->
            let
                form = model.actionForm
                newForm = { form | gridX = String.fromInt c, gridY = String.fromInt r }
            in
            ( { model | viewMode = JournalView, actionForm = newForm, editingAction = Nothing }, Cmd.none )

        SelectPaletteSpecies sid ->
            ( { model
                | paletteSpecies =
                    if model.paletteSpecies == Just sid then Nothing else Just sid
              }
            , Cmd.none
            )

        ClearPalette ->
            ( { model | paletteSpecies = Nothing }, Cmd.none )

        PlaceAtPixel px py ->
            case model.movingPlant of
                Just id ->
                    ( { model | movingPlant = Nothing }
                    , moveActionCmd model id px py
                    )
                Nothing ->
                    case model.paletteSpecies of
                        Just sid ->
                            case validatePlacement model sid px py of
                                Err errMsg ->
                                    ( { model | error = Just errMsg }, Cmd.none )

                                Ok () ->
                                    let
                                        kindToUse =
                                            case model.calendar of
                                                Just cal ->
                                                    let
                                                        doy = isoToDoy model.today
                                                        openDirect =
                                                            cal.species
                                                                |> List.filter (\sl -> sl.species.id == sid)
                                                                |> List.head
                                                                |> Maybe.andThen (\sl -> sl.directSowLocal)
                                                                |> Maybe.map (doyInWindow doy)
                                                                |> Maybe.withDefault False
                                                    in
                                                    if openDirect then "semis_direct" else "repiquage"

                                                Nothing ->
                                                    "semis_direct"

                                        newForm =
                                            { date = model.today
                                            , parcelId = ""
                                            , speciesId = sid
                                            , kind = kindToUse
                                            , quantity = ""
                                            , notes = ""
                                            , gridX = String.fromInt px
                                            , gridY = String.fromInt py
                                            }
                                    in
                                    ( { model | error = Nothing }, createAction model.backendUrl newForm )

                        Nothing ->
                            ( model, Cmd.none )

        PlaceInShelter px py ->
            case model.movingPlant of
                Just id ->
                    ( { model | movingPlant = Nothing }
                    , moveActionCmd model id px py
                    )
                Nothing ->
                    case model.paletteSpecies of
                        Just sid ->
                            let
                                newForm =
                                    { date = model.today
                                    , parcelId = ""
                                    , speciesId = sid
                                    , kind = "semis_abri"
                                    , quantity = ""
                                    , notes = ""
                                    , gridX = String.fromInt px
                                    , gridY = String.fromInt py
                                    }
                            in
                            ( model, createAction model.backendUrl newForm )
                        Nothing -> ( model, Cmd.none )

        StartMoving id ->
            ( { model | movingPlant = Just id, paletteSpecies = Nothing }, Cmd.none )

        CancelMoving ->
            ( { model | movingPlant = Nothing }, Cmd.none )

        DragStart id zone x y ->
            ( { model
                | dragging = Just { id = id, fromZone = zone, currentX = x, currentY = y, currentZone = zone, moved = False }
                , paletteSpecies = Nothing
                , movingPlant = Nothing
                , plantMenu = Nothing
              }
            , Cmd.none
            )

        DragMoveIn zone x y ->
            case model.dragging of
                Just d ->
                    let
                        hasMoved =
                            d.moved
                                || abs (x - d.currentX) > 3
                                || abs (y - d.currentY) > 3
                    in
                    ( { model | dragging = Just { d | currentX = x, currentY = y, currentZone = zone, moved = hasMoved } }, Cmd.none )
                Nothing -> ( model, Cmd.none )

        DragEnterZone zone ->
            case model.dragging of
                Just d ->
                    ( { model | dragging = Just { d | currentZone = zone, moved = True } }, Cmd.none )
                Nothing -> ( model, Cmd.none )

        DragEnd ->
            case model.dragging of
                Just d ->
                    if not d.moved then
                        -- clic simple sans drag → ouvre le menu contextuel
                        ( { model | dragging = Nothing, plantMenu = Just d.id }, Cmd.none )
                    else
                        let
                            cmd =
                                if d.fromZone == d.currentZone then
                                    moveActionCmd model d.id d.currentX d.currentY
                                else
                                    dropAcrossZonesCmd model d
                        in
                        ( { model | dragging = Nothing }, cmd )

                Nothing -> ( model, Cmd.none )

        OpenPlantMenu id ->
            ( { model | plantMenu = Just id }, Cmd.none )

        ClosePlantMenu ->
            ( { model | plantMenu = Nothing, noteDraft = "" }, Cmd.none )

        QuickAction plantId kind ->
            ( { model | plantMenu = Nothing }
            , quickActionCmd model plantId kind
            )

        SetNoteDraft s ->
            ( { model | noteDraft = s }, Cmd.none )

        SetAlmanacSearch s ->
            ( { model | almanacSearch = s }, Cmd.none )

        GotProblems (Ok ps) ->
            ( { model | problems = ps }, Cmd.none )

        GotProblems (Err e) ->
            ( { model | error = Just (httpErr e) }, Cmd.none )

        OpenProblemForm msid maid ->
            ( { model
                | newProblem =
                    Just
                        { speciesId = msid |> Maybe.withDefault ""
                        , actionId = maid
                        , title = ""
                        , category = "maladie"
                        , firstObs = ""
                        }
                , viewMode = AlmanacView
                , plantMenu = Nothing
              }
            , Cmd.none
            )

        SetProblemSpecies s ->
            ( { model | newProblem = model.newProblem |> Maybe.map (\d -> { d | speciesId = s }) }, Cmd.none )

        SetProblemTitle s ->
            ( { model | newProblem = model.newProblem |> Maybe.map (\d -> { d | title = s }) }, Cmd.none )

        SetProblemCategory s ->
            ( { model | newProblem = model.newProblem |> Maybe.map (\d -> { d | category = s }) }, Cmd.none )

        SetProblemObs s ->
            ( { model | newProblem = model.newProblem |> Maybe.map (\d -> { d | firstObs = s }) }, Cmd.none )

        CancelProblemForm ->
            ( { model | newProblem = Nothing }, Cmd.none )

        SubmitProblem ->
            case model.newProblem of
                Just d ->
                    if String.trim d.title == "" then
                        ( model, Cmd.none )
                    else
                        ( model, createProblemCmd model.backendUrl d )

                Nothing -> ( model, Cmd.none )

        GotProblemCreated (Ok pid) ->
            let
                obs =
                    model.newProblem
                        |> Maybe.map (.firstObs >> String.trim)
                        |> Maybe.withDefault ""
            in
            ( { model | newProblem = Nothing }
            , if obs == "" then
                fetchProblems model.backendUrl
              else
                createEntryCmd model.backendUrl pid model.today "observation" obs
            )

        GotProblemCreated (Err e) ->
            ( { model | error = Just (httpErr e) }, Cmd.none )

        StartEntry pid kind ->
            ( { model | entryDraft = Just { problemId = pid, kind = kind, text = "" } }, Cmd.none )

        SetEntryText s ->
            ( { model | entryDraft = model.entryDraft |> Maybe.map (\d -> { d | text = s }) }, Cmd.none )

        CancelEntry ->
            ( { model | entryDraft = Nothing }, Cmd.none )

        SubmitEntry ->
            case model.entryDraft of
                Just ed ->
                    if String.trim ed.text == "" then
                        ( model, Cmd.none )
                    else
                        ( { model | entryDraft = Nothing }
                        , createEntryCmd model.backendUrl ed.problemId model.today ed.kind (String.trim ed.text)
                        )

                Nothing -> ( model, Cmd.none )

        StartClose pid ->
            ( { model | closeDraft = Just ( pid, "" ) }, Cmd.none )

        SetCloseText s ->
            ( { model | closeDraft = model.closeDraft |> Maybe.map (\( pid, _ ) -> ( pid, s )) }, Cmd.none )

        CancelClose ->
            ( { model | closeDraft = Nothing }, Cmd.none )

        SubmitClose ->
            case model.closeDraft of
                Just ( pid, conclusion ) ->
                    if String.trim conclusion == "" then
                        ( model, Cmd.none )
                    else
                        ( { model | closeDraft = Nothing }
                        , closeProblemCmd model.backendUrl pid (String.trim conclusion)
                        )

                Nothing -> ( model, Cmd.none )

        ReopenProblem pid ->
            ( model, reopenProblemCmd model.backendUrl pid )

        DeleteProblem pid ->
            ( model, deleteProblemCmd model.backendUrl pid )

        GotProblemSaved (Ok _) ->
            ( model, fetchProblems model.backendUrl )

        GotProblemSaved (Err e) ->
            ( { model | error = Just (httpErr e) }, Cmd.none )

        EditSolution noteId initial ->
            ( { model | solutionDraft = Just ( noteId, initial ) }, Cmd.none )

        SetSolutionDraft s ->
            ( { model
                | solutionDraft =
                    model.solutionDraft |> Maybe.map (\( nid, _ ) -> ( nid, s ))
              }
            , Cmd.none
            )

        CancelSolution ->
            ( { model | solutionDraft = Nothing }, Cmd.none )

        SaveSolution noteId ->
            case model.solutionDraft of
                Just ( nid, txt ) ->
                    if nid == noteId then
                        ( { model | solutionDraft = Nothing }
                        , saveSolutionCmd model noteId txt
                        )
                    else
                        ( model, Cmd.none )

                Nothing -> ( model, Cmd.none )

        GotSolutionSaved (Ok _) ->
            ( model, fetchActions model.backendUrl )

        GotSolutionSaved (Err e) ->
            ( { model | error = Just (httpErr e) }, Cmd.none )

        SaveObservation plantId ->
            if String.trim model.noteDraft == "" then
                ( model, Cmd.none )
            else
                ( { model | plantMenu = Nothing, noteDraft = "" }
                , observationCmd model plantId model.noteDraft
                )

        SetBulkKind k -> ( { model | bulkKind = k }, Cmd.none )
        SetBulkSpecies s -> ( { model | bulkSpeciesId = toMaybe s }, Cmd.none )
        SetBulkZone s ->
            ( { model
                | bulkZone =
                    case s of
                        "shelter" -> Just Shelter
                        "terrain" -> Just Terrain
                        _ -> Nothing
              }
            , Cmd.none
            )
        ToggleBulkMature -> ( { model | bulkOnlyMature = not model.bulkOnlyMature }, Cmd.none )

        ApplyBulk ->
            ( model, applyBulkCmd model )

        TerrainCursorMove x y ->
            case model.panning of
                Just pan ->
                    let
                        gv = model.gardenView
                        dx = toFloat (x - pan.startMouseX) / gv.zoom
                        dy = toFloat (y - pan.startMouseY) / gv.zoom
                        newPanX = clamp 0 (800 - 800 / gv.zoom) (pan.startPanX - dx)
                        newPanY = clamp 0 (560 - 560 / gv.zoom) (pan.startPanY - dy)
                    in
                    ( { model | gardenView = { gv | panX = newPanX, panY = newPanY } }, Cmd.none )
                Nothing ->
                    let
                        newDragging =
                            case model.dragging of
                                Just d ->
                                    let
                                        hasMoved =
                                            d.moved
                                                || abs (x - d.currentX) > 3
                                                || abs (y - d.currentY) > 3
                                    in
                                    Just { d | currentX = x, currentY = y, currentZone = Terrain, moved = hasMoved }
                                Nothing -> Nothing
                    in
                    ( { model | cursorOnTerrain = Just ( x, y ), dragging = newDragging }, Cmd.none )

        TerrainCursorLeave ->
            ( { model | cursorOnTerrain = Nothing }, Cmd.none )

        RequestClearAll ->
            ( { model | confirmingClearAll = True }, Cmd.none )

        CancelClearAll ->
            ( { model | confirmingClearAll = False }, Cmd.none )

        ConfirmClearAll ->
            ( { model | confirmingClearAll = False }
            , clearAllActionsCmd model.backendUrl
            )

        SetViewDoy n ->
            ( { model | viewDoy = clamp 1 365 n }, Cmd.none )

        ResetViewDoy ->
            ( { model | viewDoy = isoToDoy model.today }, Cmd.none )

        GardenZoom dy ox oy ->
            let
                gv = model.gardenView
                factor = if dy < 0 then 1.15 else 1 / 1.15
                newZoom = clamp 1 6 (gv.zoom * factor)
                cursorSvgX = gv.panX + toFloat ox / gv.zoom
                cursorSvgY = gv.panY + toFloat oy / gv.zoom
                newPanX = cursorSvgX - toFloat ox / newZoom
                newPanY = cursorSvgY - toFloat oy / newZoom
                clampedPanX = clamp 0 (800 - 800 / newZoom) newPanX
                clampedPanY = clamp 0 (560 - 560 / newZoom) newPanY
            in
            ( { model | gardenView = { zoom = newZoom, panX = clampedPanX, panY = clampedPanY } }, Cmd.none )

        GardenZoomReset ->
            ( { model | gardenView = { zoom = 1, panX = 0, panY = 0 } }, Cmd.none )

        GardenPanStart altKey x y ->
            if altKey then
                let gv = model.gardenView in
                ( { model | panning = Just { startMouseX = x, startMouseY = y, startPanX = gv.panX, startPanY = gv.panY } }, Cmd.none )
            else
                ( model, Cmd.none )

        GardenPanEnd ->
            ( { model | panning = Nothing }, Cmd.none )

        HoverPlant mid ->
            ( { model | hoverPlant = mid }, Cmd.none )

        DeletePlant id ->
            ( model, deleteAction model.backendUrl id )

        RefreshClimate ->
            ( { model | refreshingClimate = True, error = Nothing }
            , fetchCalendar model.backendUrl model.selectedCity True
            )

        SetFilterCategory cat -> ( { model | filterCategory = toMaybe cat }, Cmd.none )
        SetFilterDifficulty d -> ( { model | filterDifficulty = toMaybe d }, Cmd.none )
        SetSearch s -> ( { model | search = s }, Cmd.none )
        SelectSpeciesRow id ->
            ( { model | selectedSpecies = if model.selectedSpecies == Just id then Nothing else Just id }, Cmd.none )

        SetViewMode vm -> ( { model | viewMode = vm }, Cmd.none )

        GotParcels (Ok ps) -> ( { model | parcels = ps }, Cmd.none )
        GotParcels (Err e) -> ( { model | error = Just (httpErr e) }, Cmd.none )
        GotActions (Ok xs) -> ( { model | actions = xs }, Cmd.none )
        GotActions (Err e) -> ( { model | error = Just (httpErr e) }, Cmd.none )
        GotActionKinds (Ok ks) -> ( { model | actionKinds = ks }, Cmd.none )
        GotActionKinds (Err _) -> ( model, Cmd.none )

        GotParcelSaved (Ok _) ->
            ( { model | parcelForm = emptyParcelForm, editingParcel = Nothing }
            , fetchParcels model.backendUrl
            )
        GotParcelSaved (Err e) -> ( { model | error = Just (httpErr e) }, Cmd.none )

        GotActionSaved (Ok _) ->
            ( { model | actionForm = emptyActionForm model.actionForm.date, editingAction = Nothing }
            , fetchActions model.backendUrl
            )
        GotActionSaved (Err e) -> ( { model | error = Just (httpErr e) }, Cmd.none )

        GotBulkSaved (Ok _) ->
            ( model, fetchActions model.backendUrl )
        GotBulkSaved (Err e) -> ( { model | error = Just (httpErr e) }, Cmd.none )

        GotDeleted (Ok _) ->
            ( model
            , Cmd.batch
                [ fetchParcels model.backendUrl
                , fetchActions model.backendUrl
                ]
            )
        GotDeleted (Err e) -> ( { model | error = Just (httpErr e) }, Cmd.none )

        SetParcelName s -> ( { model | parcelForm = setPN model.parcelForm s }, Cmd.none )
        SetParcelSurface s -> ( { model | parcelForm = setPS model.parcelForm s }, Cmd.none )
        SetParcelExposition s -> ( { model | parcelForm = setPE model.parcelForm s }, Cmd.none )
        SetParcelSoilNotes s -> ( { model | parcelForm = setPSN model.parcelForm s }, Cmd.none )
        SetParcelGridX s -> ( { model | parcelForm = (\f -> { f | gridX = s }) model.parcelForm }, Cmd.none )
        SetParcelGridY s -> ( { model | parcelForm = (\f -> { f | gridY = s }) model.parcelForm }, Cmd.none )
        SetParcelGridW s -> ( { model | parcelForm = (\f -> { f | gridW = s }) model.parcelForm }, Cmd.none )
        SetParcelGridH s -> ( { model | parcelForm = (\f -> { f | gridH = s }) model.parcelForm }, Cmd.none )
        SetParcelColor s -> ( { model | parcelForm = (\f -> { f | color = s }) model.parcelForm }, Cmd.none )

        SubmitParcel ->
            if String.trim model.parcelForm.name == "" then
                ( { model | error = Just "Nom de parcelle obligatoire" }, Cmd.none )

            else
                ( model
                , case model.editingParcel of
                    Just id -> updateParcel model.backendUrl id model.parcelForm
                    Nothing -> createParcel model.backendUrl model.parcelForm
                )

        EditParcel p ->
            ( { model
                | editingParcel = Just p.id
                , parcelForm =
                    { name = p.name
                    , surface = p.surfaceM2 |> Maybe.map String.fromFloat |> Maybe.withDefault ""
                    , exposition = p.exposition |> Maybe.withDefault ""
                    , soilNotes = p.soilNotes |> Maybe.withDefault ""
                    , gridX = String.fromInt p.gridX
                    , gridY = String.fromInt p.gridY
                    , gridW = String.fromInt p.gridW
                    , gridH = String.fromInt p.gridH
                    , color = p.color
                    }
              }
            , Cmd.none
            )

        CancelEditParcel ->
            ( { model | editingParcel = Nothing, parcelForm = emptyParcelForm }, Cmd.none )

        DeleteParcel id ->
            ( model, deleteParcel model.backendUrl id )

        SetActionDate s -> ( { model | actionForm = setAD model.actionForm s }, Cmd.none )
        SetActionParcel s -> ( { model | actionForm = setAP model.actionForm s }, Cmd.none )
        SetActionSpecies s -> ( { model | actionForm = setAS model.actionForm s }, Cmd.none )
        SetActionKind s -> ( { model | actionForm = setAK model.actionForm s }, Cmd.none )
        SetActionQty s -> ( { model | actionForm = setAQ model.actionForm s }, Cmd.none )
        SetActionNotes s -> ( { model | actionForm = setAN model.actionForm s }, Cmd.none )

        SubmitAction ->
            if String.trim model.actionForm.date == "" then
                ( { model | error = Just "Date obligatoire" }, Cmd.none )

            else
                ( model
                , case model.editingAction of
                    Just id -> updateAction model.backendUrl id model.actionForm
                    Nothing -> createAction model.backendUrl model.actionForm
                )

        EditAction a ->
            ( { model
                | editingAction = Just a.id
                , actionForm =
                    { date = a.date
                    , parcelId = a.parcelId |> Maybe.map String.fromInt |> Maybe.withDefault ""
                    , speciesId = a.speciesId |> Maybe.withDefault ""
                    , kind = a.kind
                    , quantity = a.quantityG |> Maybe.map String.fromFloat |> Maybe.withDefault ""
                    , notes = a.notes |> Maybe.withDefault ""
                    , gridX = a.gridX |> Maybe.map String.fromInt |> Maybe.withDefault ""
                    , gridY = a.gridY |> Maybe.map String.fromInt |> Maybe.withDefault ""
                    }
              }
            , Cmd.none
            )

        CancelEditAction ->
            ( { model | editingAction = Nothing, actionForm = emptyActionForm model.actionForm.date }, Cmd.none )

        DeleteAction id ->
            ( model, deleteAction model.backendUrl id )

        SetFilterActionParcel s ->
            ( { model | filterActionParcel = String.toInt s }, Cmd.none )

        SetFilterActionKind s ->
            ( { model | filterActionKind = toMaybe s }, Cmd.none )



-- Parcel form setters


setPN : ParcelForm -> String -> ParcelForm
setPN f s = { f | name = s }
setPS : ParcelForm -> String -> ParcelForm
setPS f s = { f | surface = s }
setPE : ParcelForm -> String -> ParcelForm
setPE f s = { f | exposition = s }
setPSN : ParcelForm -> String -> ParcelForm
setPSN f s = { f | soilNotes = s }


-- Action form setters


setAD : ActionForm -> String -> ActionForm
setAD f s = { f | date = s }
setAP : ActionForm -> String -> ActionForm
setAP f s = { f | parcelId = s }
setAS : ActionForm -> String -> ActionForm
setAS f s = { f | speciesId = s }
setAK : ActionForm -> String -> ActionForm
setAK f s = { f | kind = s }
setAQ : ActionForm -> String -> ActionForm
setAQ f s = { f | quantity = s }
setAN : ActionForm -> String -> ActionForm
setAN f s = { f | notes = s }


toMaybe : String -> Maybe String
toMaybe s = if s == "" then Nothing else Just s


httpErr : Http.Error -> String
httpErr e =
    case e of
        Http.BadUrl s -> "URL invalide : " ++ s
        Http.Timeout -> "Timeout"
        Http.NetworkError -> "Réseau injoignable — backend démarré ?"
        Http.BadStatus code -> "HTTP " ++ String.fromInt code
        Http.BadBody m -> "JSON invalide : " ++ m



-- HTTP


fetchCities : String -> Cmd Msg
fetchCities url =
    Http.get
        { url = url ++ "/cities"
        , expect = Http.expectJson GotCities (D.list cityDecoder)
        }


fetchCalendar : String -> String -> Bool -> Cmd Msg
fetchCalendar url slug refresh =
    let q = "?city=" ++ slug ++ (if refresh then "&refresh_climate=true" else "") in
    Http.get
        { url = url ++ "/calendar" ++ q
        , expect = Http.expectJson GotCalendar calendarResponseDecoder
        }


fetchParcels : String -> Cmd Msg
fetchParcels url =
    Http.get
        { url = url ++ "/parcels"
        , expect = Http.expectJson GotParcels (D.list parcelDecoder)
        }


fetchActions : String -> Cmd Msg
fetchActions url =
    Http.get
        { url = url ++ "/actions?limit=5000"
        , expect = Http.expectJson GotActions (D.list actionDecoder)
        }


fetchProblems : String -> Cmd Msg
fetchProblems url =
    Http.get
        { url = url ++ "/problems"
        , expect = Http.expectJson GotProblems (D.list problemDecoder)
        }


createProblemCmd : String -> NewProblemDraft -> Cmd Msg
createProblemCmd url draft =
    Http.post
        { url = url ++ "/problems"
        , body =
            Http.jsonBody
                (Encode.object
                    [ ( "species_id", encodeMaybeString draft.speciesId )
                    , ( "action_id", encodeMaybeInt (draft.actionId |> Maybe.map String.fromInt |> Maybe.withDefault "") )
                    , ( "title", Encode.string draft.title )
                    , ( "category", Encode.string draft.category )
                    ]
                )
        , expect = Http.expectJson GotProblemCreated D.int
        }


createEntryCmd : String -> Int -> String -> String -> String -> Cmd Msg
createEntryCmd url problemId date kind text_ =
    Http.post
        { url = url ++ "/problems/" ++ String.fromInt problemId ++ "/entries"
        , body =
            Http.jsonBody
                (Encode.object
                    [ ( "date", Encode.string date )
                    , ( "kind", Encode.string kind )
                    , ( "text", Encode.string text_ )
                    ]
                )
        , expect = Http.expectWhatever GotProblemSaved
        }


closeProblemCmd : String -> Int -> String -> Cmd Msg
closeProblemCmd url problemId conclusion =
    Http.request
        { method = "PUT"
        , headers = []
        , url = url ++ "/problems/" ++ String.fromInt problemId
        , body =
            Http.jsonBody
                (Encode.object
                    [ ( "status", Encode.string "resolved" )
                    , ( "conclusion", Encode.string conclusion )
                    ]
                )
        , expect = Http.expectWhatever GotProblemSaved
        , timeout = Nothing
        , tracker = Nothing
        }


reopenProblemCmd : String -> Int -> Cmd Msg
reopenProblemCmd url problemId =
    Http.request
        { method = "PUT"
        , headers = []
        , url = url ++ "/problems/" ++ String.fromInt problemId
        , body = Http.jsonBody (Encode.object [ ( "status", Encode.string "open" ) ])
        , expect = Http.expectWhatever GotProblemSaved
        , timeout = Nothing
        , tracker = Nothing
        }


deleteProblemCmd : String -> Int -> Cmd Msg
deleteProblemCmd url problemId =
    Http.request
        { method = "DELETE"
        , headers = []
        , url = url ++ "/problems/" ++ String.fromInt problemId
        , body = Http.emptyBody
        , expect = Http.expectWhatever GotProblemSaved
        , timeout = Nothing
        , tracker = Nothing
        }


problemDecoder : Decoder Problem
problemDecoder =
    D.succeed Problem
        |> andMap (D.field "id" D.int)
        |> andMap (D.field "species_id" (D.nullable D.string))
        |> andMap (D.field "action_id" (D.nullable D.int))
        |> andMap (D.field "title" D.string)
        |> andMap (D.field "category" D.string)
        |> andMap (D.field "status" D.string)
        |> andMap (D.field "conclusion" (D.nullable D.string))
        |> andMap (D.field "entries" (D.list problemEntryDecoder))


problemEntryDecoder : Decoder ProblemEntry
problemEntryDecoder =
    D.succeed ProblemEntry
        |> andMap (D.field "id" D.int)
        |> andMap (D.field "problem_id" D.int)
        |> andMap (D.field "date" D.string)
        |> andMap (D.field "kind" D.string)
        |> andMap (D.field "text" D.string)


fetchActionKinds : String -> Cmd Msg
fetchActionKinds url =
    Http.get
        { url = url ++ "/action-kinds"
        , expect = Http.expectJson GotActionKinds (D.list D.string)
        }


fetchForecast : String -> String -> Cmd Msg
fetchForecast url slug =
    Http.get
        { url = url ++ "/forecast?city=" ++ slug
        , expect = Http.expectJson GotForecast (D.list forecastDayDecoder)
        }


fetchHistorical : String -> String -> Cmd Msg
fetchHistorical url slug =
    Http.get
        { url = url ++ "/historical-year?city=" ++ slug
        , expect = Http.expectJson GotHistorical (D.list historicalDayDecoder)
        }


historicalDayDecoder : Decoder HistoricalDay
historicalDayDecoder =
    D.map5 HistoricalDay
        (D.field "doy" D.int)
        (D.field "temp_min_c" D.float)
        (D.field "temp_max_c" D.float)
        (D.field "precipitation_mm" D.float)
        (D.field "samples" D.int)


forecastDayDecoder : Decoder ForecastDay
forecastDayDecoder =
    D.map6 ForecastDay
        (D.field "date" D.string)
        (D.field "temp_min_c" D.float)
        (D.field "temp_max_c" D.float)
        (D.field "precipitation_mm" D.float)
        (D.field "wind_kmh" D.float)
        (D.field "kind" D.string)


encodeParcel : ParcelForm -> Encode.Value
encodeParcel f =
    Encode.object
        [ ( "name", Encode.string f.name )
        , ( "surface_m2", encodeMaybeFloat f.surface )
        , ( "exposition", encodeMaybeString f.exposition )
        , ( "soil_notes", encodeMaybeString f.soilNotes )
        , ( "grid_x", encodeMaybeInt f.gridX )
        , ( "grid_y", encodeMaybeInt f.gridY )
        , ( "grid_w", encodeMaybeInt f.gridW )
        , ( "grid_h", encodeMaybeInt f.gridH )
        , ( "color", Encode.string f.color )
        ]


encodeAction : ActionForm -> Encode.Value
encodeAction f =
    Encode.object
        [ ( "date", Encode.string f.date )
        , ( "parcel_id", encodeMaybeInt f.parcelId )
        , ( "species_id", encodeMaybeString f.speciesId )
        , ( "kind", Encode.string f.kind )
        , ( "quantity_g", encodeMaybeFloat f.quantity )
        , ( "notes", encodeMaybeString f.notes )
        , ( "grid_x", encodeMaybeInt f.gridX )
        , ( "grid_y", encodeMaybeInt f.gridY )
        ]


encodeMaybeString : String -> Encode.Value
encodeMaybeString s =
    if String.trim s == "" then Encode.null else Encode.string (String.trim s)


encodeMaybeFloat : String -> Encode.Value
encodeMaybeFloat s =
    case String.toFloat (String.trim s) of
        Just v -> Encode.float v
        Nothing -> Encode.null


encodeMaybeInt : String -> Encode.Value
encodeMaybeInt s =
    case String.toInt (String.trim s) of
        Just v -> Encode.int v
        Nothing -> Encode.null


createParcel : String -> ParcelForm -> Cmd Msg
createParcel url form =
    Http.post
        { url = url ++ "/parcels"
        , body = Http.jsonBody (encodeParcel form)
        , expect = Http.expectJson GotParcelSaved parcelDecoder
        }


updateParcel : String -> Int -> ParcelForm -> Cmd Msg
updateParcel url id form =
    Http.request
        { method = "PUT"
        , headers = []
        , url = url ++ "/parcels/" ++ String.fromInt id
        , body = Http.jsonBody (encodeParcel form)
        , expect = Http.expectJson GotParcelSaved parcelDecoder
        , timeout = Nothing
        , tracker = Nothing
        }


deleteParcel : String -> Int -> Cmd Msg
deleteParcel url id =
    Http.request
        { method = "DELETE"
        , headers = []
        , url = url ++ "/parcels/" ++ String.fromInt id
        , body = Http.emptyBody
        , expect = Http.expectWhatever GotDeleted
        , timeout = Nothing
        , tracker = Nothing
        }


createAction : String -> ActionForm -> Cmd Msg
createAction url form =
    Http.post
        { url = url ++ "/actions"
        , body = Http.jsonBody (encodeAction form)
        , expect = Http.expectJson GotActionSaved actionDecoder
        }


createBulkActions : String -> List ActionForm -> Cmd Msg
createBulkActions url forms =
    Http.post
        { url = url ++ "/actions/bulk"
        , body = Http.jsonBody (Encode.list encodeAction forms)
        , expect = Http.expectWhatever GotBulkSaved
        }


updateAction : String -> Int -> ActionForm -> Cmd Msg
updateAction url id form =
    Http.request
        { method = "PUT"
        , headers = []
        , url = url ++ "/actions/" ++ String.fromInt id
        , body = Http.jsonBody (encodeAction form)
        , expect = Http.expectJson GotActionSaved actionDecoder
        , timeout = Nothing
        , tracker = Nothing
        }


-- Enregistre la solution d'une note : PUT complet reconstruit depuis
-- l'ActionEntry existante pour ne perdre aucun champ.
saveSolutionCmd : Model -> Int -> String -> Cmd Msg
saveSolutionCmd model noteId solutionText =
    case model.actions |> List.filter (\a -> a.id == noteId) |> List.head of
        Just a ->
            Http.request
                { method = "PUT"
                , headers = []
                , url = model.backendUrl ++ "/actions/" ++ String.fromInt noteId
                , body = Http.jsonBody (encodeEntryWithSolution a solutionText)
                , expect = Http.expectWhatever GotSolutionSaved
                , timeout = Nothing
                , tracker = Nothing
                }

        Nothing -> Cmd.none


encodeEntryWithSolution : ActionEntry -> String -> Encode.Value
encodeEntryWithSolution a sol =
    let
        maybeOr enc m = m |> Maybe.map enc |> Maybe.withDefault Encode.null
    in
    Encode.object
        [ ( "date", Encode.string a.date )
        , ( "parcel_id", maybeOr Encode.int a.parcelId )
        , ( "species_id", maybeOr Encode.string a.speciesId )
        , ( "kind", Encode.string a.kind )
        , ( "quantity_g", maybeOr Encode.float a.quantityG )
        , ( "notes", maybeOr Encode.string a.notes )
        , ( "grid_x", maybeOr Encode.int a.gridX )
        , ( "grid_y", maybeOr Encode.int a.gridY )
        , ( "solution", Encode.string (String.trim sol) )
        ]


deleteAction : String -> Int -> Cmd Msg
deleteAction url id =
    Http.request
        { method = "DELETE"
        , headers = []
        , url = url ++ "/actions/" ++ String.fromInt id
        , body = Http.emptyBody
        , expect = Http.expectWhatever GotDeleted
        , timeout = Nothing
        , tracker = Nothing
        }


clearAllActionsCmd : String -> Cmd Msg
clearAllActionsCmd url =
    Http.request
        { method = "DELETE"
        , headers = []
        , url = url ++ "/actions"
        , body = Http.emptyBody
        , expect = Http.expectWhatever GotDeleted
        , timeout = Nothing
        , tracker = Nothing
        }


-- Drop d'un plant d'une zone à l'autre. Si abri→terrain : création repiquage
-- + suppression abri. Si terrain→abri : conversion en semis_abri.
dropAcrossZonesCmd : Model -> DragState -> Cmd Msg
dropAcrossZonesCmd model d =
    case model.actions |> List.filter (\a -> a.id == d.id) |> List.head of
        Just a ->
            let
                sid = a.speciesId |> Maybe.withDefault ""
                newKind =
                    case d.currentZone of
                        Terrain -> "repiquage"
                        Shelter -> "semis_abri"
                newForm =
                    -- Date = date du semis d'origine (a.date), pas today :
                    -- la progression (daysToHarvest compte depuis le semis)
                    -- doit inclure le temps passé en pépinière.
                    { date = a.date
                    , parcelId = ""
                    , speciesId = sid
                    , kind = newKind
                    , quantity = ""
                    , notes =
                        case ( d.fromZone, d.currentZone ) of
                            ( Shelter, Terrain ) -> "repiqué depuis pépinière le " ++ model.today
                            ( Terrain, Shelter ) -> "replacé sous abri le " ++ model.today
                            _ -> ""
                    , gridX = String.fromInt d.currentX
                    , gridY = String.fromInt d.currentY
                    }
            in
            Cmd.batch
                [ createAction model.backendUrl newForm
                , deleteAction model.backendUrl d.id
                ]
        Nothing -> Cmd.none


-- Validation du placement : espacement minimum + voisinage antagoniste.
-- Échelle : 1 px = 1 cm sur le terrain.
validatePlacement : Model -> String -> Int -> Int -> Result String ()
validatePlacement model speciesId px py =
    case findSpecies speciesId model of
        Nothing -> Ok ()
        Just sp ->
            let
                others = plantsFromActions model
                spacing = sp.spacingCm
                foeRadius = max 30 (spacing * 2)
                neighborSpacing pl =
                    case findSpecies pl.speciesId model of
                        Just other -> (spacing + other.spacingCm) // 2
                        Nothing -> spacing
                -- Compagnon : 70% de l'espacement standard (les racines/feuillages
                -- s'entraident, on peut les serrer un peu plus).
                friendSpacing pl = neighborSpacing pl * 7 // 10
                conflictSame =
                    others
                        |> List.filter
                            (\pl ->
                                pl.speciesId == speciesId
                                    && distanceTo px py pl < spacing
                            )
                        |> List.head
                conflictFoe =
                    others
                        |> List.filter
                            (\pl ->
                                List.member pl.speciesId sp.foes
                                    && distanceTo px py pl < foeRadius
                            )
                        |> List.head
                conflictFriend =
                    others
                        |> List.filter
                            (\pl ->
                                List.member pl.speciesId sp.friends
                                    && distanceTo px py pl < friendSpacing pl
                            )
                        |> List.head
                conflictNeutral =
                    others
                        |> List.filter
                            (\pl ->
                                pl.speciesId /= speciesId
                                    && not (List.member pl.speciesId sp.foes)
                                    && not (List.member pl.speciesId sp.friends)
                                    && distanceTo px py pl < neighborSpacing pl
                            )
                        |> List.head
            in
            case ( ( conflictSame, conflictFoe ), ( conflictFriend, conflictNeutral ) ) of
                ( ( Just c, _ ), _ ) ->
                    Err
                        ("Trop près d'un autre "
                            ++ speciesShortName speciesId
                            ++ " ("
                            ++ String.fromInt (distanceTo px py c)
                            ++ " cm, mini "
                            ++ String.fromInt spacing
                            ++ " cm)"
                        )

                ( ( _, Just c ), _ ) ->
                    Err
                        ("Antagoniste trop proche : "
                            ++ speciesShortName c.speciesId
                            ++ " à "
                            ++ String.fromInt (distanceTo px py c)
                            ++ " cm. Éloigne-toi d'au moins "
                            ++ String.fromInt foeRadius
                            ++ " cm."
                        )

                ( _, ( Just c, _ ) ) ->
                    Err
                        ("Trop près d'un compagnon "
                            ++ speciesShortName c.speciesId
                            ++ " ("
                            ++ String.fromInt (distanceTo px py c)
                            ++ " cm, mini "
                            ++ String.fromInt (friendSpacing c)
                            ++ " cm — compagnonnage tolère 70% de l'espacement)"
                        )

                ( _, ( _, Just c ) ) ->
                    Err
                        ("Trop près d'un "
                            ++ speciesShortName c.speciesId
                            ++ " ("
                            ++ String.fromInt (distanceTo px py c)
                            ++ " cm, mini "
                            ++ String.fromInt (neighborSpacing c)
                            ++ " cm — moyenne des espacements)"
                        )

                _ ->
                    Ok ()


findSpecies : String -> Model -> Maybe Species
findSpecies sid model =
    case model.calendar of
        Just cal ->
            cal.species |> List.filter (\sl -> sl.species.id == sid) |> List.head |> Maybe.map .species
        Nothing -> Nothing


distanceTo : Int -> Int -> PlantOnTerrain -> Int
distanceTo px py pl =
    let
        dx = toFloat (px - pl.x)
        dy = toFloat (py - pl.y)
    in
    round (sqrt (dx * dx + dy * dy))


quickActionCmd : Model -> Int -> String -> Cmd Msg
quickActionCmd model plantId kind =
    case model.actions |> List.filter (\a -> a.id == plantId) |> List.head of
        Just a ->
            let
                form =
                    { date = model.today
                    , parcelId = ""
                    , speciesId = a.speciesId |> Maybe.withDefault ""
                    , kind = kind
                    , quantity = ""
                    , notes = ""
                    , gridX = a.gridX |> Maybe.map String.fromInt |> Maybe.withDefault ""
                    , gridY = a.gridY |> Maybe.map String.fromInt |> Maybe.withDefault ""
                    }
            in
            createAction model.backendUrl form
        Nothing -> Cmd.none


observationCmd : Model -> Int -> String -> Cmd Msg
observationCmd model plantId noteText =
    case model.actions |> List.filter (\a -> a.id == plantId) |> List.head of
        Just a ->
            createAction model.backendUrl
                { date = model.today
                , parcelId = ""
                , speciesId = a.speciesId |> Maybe.withDefault ""
                , kind = "note"
                , quantity = ""
                , notes = noteText
                , gridX = a.gridX |> Maybe.map String.fromInt |> Maybe.withDefault ""
                , gridY = a.gridY |> Maybe.map String.fromInt |> Maybe.withDefault ""
                }
        Nothing -> Cmd.none


applyBulkCmd : Model -> Cmd Msg
applyBulkCmd model =
    let
        selected = bulkSelection model
        mk plant =
            { date = model.today
            , parcelId = ""
            , speciesId = plant.speciesId
            , kind = model.bulkKind
            , quantity = ""
            , notes = "lot"
            , gridX = String.fromInt plant.x
            , gridY = String.fromInt plant.y
            }
    in
    selected
        |> List.map mk
        |> createBulkActions model.backendUrl


bulkSelection : Model -> List PlantOnTerrain
bulkSelection model =
    let
        all =
            case model.bulkZone of
                Just Shelter -> shelterPlantsFromActions model
                Just Terrain -> plantsFromActions model
                Nothing -> plantsFromActions model ++ shelterPlantsFromActions model
    in
    all
        |> List.filter
            (\p ->
                (case model.bulkSpeciesId of
                    Just sid -> p.speciesId == sid
                    Nothing -> True
                )
                    && (if model.bulkOnlyMature then
                            case p.state of
                                TileMature _ -> True
                                _ -> False
                        else True
                       )
            )


-- Met à jour la position d'une action existante (déplacement).
moveActionCmd : Model -> Int -> Int -> Int -> Cmd Msg
moveActionCmd model id px py =
    case model.actions |> List.filter (\a -> a.id == id) |> List.head of
        Just a ->
            let
                form =
                    { date = a.date
                    , parcelId = a.parcelId |> Maybe.map String.fromInt |> Maybe.withDefault ""
                    , speciesId = a.speciesId |> Maybe.withDefault ""
                    , kind = a.kind
                    , quantity = a.quantityG |> Maybe.map String.fromFloat |> Maybe.withDefault ""
                    , notes = a.notes |> Maybe.withDefault ""
                    , gridX = String.fromInt px
                    , gridY = String.fromInt py
                    }
            in
            updateAction model.backendUrl id form
        Nothing -> Cmd.none


-- DECODERS


cityDecoder : Decoder City
cityDecoder =
    D.map4 City
        (D.field "slug" D.string)
        (D.field "name" D.string)
        (D.field "latitude" D.float)
        (D.field "longitude" D.float)


windowDecoder : Decoder CalendarWindow
windowDecoder =
    D.map2 CalendarWindow
        (D.field "doy_start" D.int)
        (D.field "doy_end" D.int)


speciesDecoder : Decoder Species
speciesDecoder =
    D.succeed Species
        |> andMap (D.field "id" D.string)
        |> andMap (D.field "name_fr" D.string)
        |> andMap (D.field "name_latin" D.string)
        |> andMap (D.field "family" D.string)
        |> andMap (D.field "life_cycle" D.string)
        |> andMap (D.field "category" D.string)
        |> andMap (D.field "difficulty" D.string)
        |> andMap (D.field "indoor_sow" (D.nullable windowDecoder))
        |> andMap (D.field "direct_sow" (D.nullable windowDecoder))
        |> andMap (D.field "transplant" (D.nullable windowDecoder))
        |> andMap (D.field "harvest" windowDecoder)
        |> andMap (D.field "depth_cm" D.float)
        |> andMap (D.field "spacing_cm" D.int)
        |> andMap (D.field "days_to_harvest" D.int)
        |> andMap (D.field "notes" (D.list D.string))
        |> andMap (D.field "friends" (D.list D.string))
        |> andMap (D.field "foes" (D.list D.string))


andMap : Decoder a -> Decoder (a -> b) -> Decoder b
andMap = D.map2 (|>)


speciesLocalDecoder : Decoder SpeciesLocal
speciesLocalDecoder =
    D.succeed SpeciesLocal
        |> andMap speciesDecoder
        |> andMap (D.field "shift_days" D.int)
        |> andMap (D.field "indoor_sow_local" (D.nullable windowDecoder))
        |> andMap (D.field "direct_sow_local" (D.nullable windowDecoder))
        |> andMap (D.field "transplant_local" (D.nullable windowDecoder))
        |> andMap (D.field "harvest_local" windowDecoder)


locationDecoder : Decoder Location
locationDecoder =
    D.map4 Location
        (D.field "name" D.string)
        (D.field "latitude" D.float)
        (D.field "longitude" D.float)
        (D.field "altitude_m" D.float)


calendarResponseDecoder : Decoder CalendarResponse
calendarResponseDecoder =
    D.map3 CalendarResponse
        (D.field "location" locationDecoder)
        (D.field "climate_source" D.string)
        (D.field "species" (D.list speciesLocalDecoder))


parcelDecoder : Decoder Parcel
parcelDecoder =
    D.succeed Parcel
        |> andMap (D.field "id" D.int)
        |> andMap (D.field "name" D.string)
        |> andMap (D.field "surface_m2" (D.nullable D.float))
        |> andMap (D.field "exposition" (D.nullable D.string))
        |> andMap (D.field "soil_notes" (D.nullable D.string))
        |> andMap (D.field "grid_x" D.int)
        |> andMap (D.field "grid_y" D.int)
        |> andMap (D.field "grid_w" D.int)
        |> andMap (D.field "grid_h" D.int)
        |> andMap (D.field "color" D.string)


actionDecoder : Decoder ActionEntry
actionDecoder =
    D.succeed ActionEntry
        |> andMap (D.field "id" D.int)
        |> andMap (D.field "date" D.string)
        |> andMap (D.field "parcel_id" (D.nullable D.int))
        |> andMap (D.field "species_id" (D.nullable D.string))
        |> andMap (D.field "kind" D.string)
        |> andMap (D.field "quantity_g" (D.nullable D.float))
        |> andMap (D.field "notes" (D.nullable D.string))
        |> andMap (D.field "grid_x" (D.nullable D.int))
        |> andMap (D.field "grid_y" (D.nullable D.int))
        |> andMap (D.field "solution" (D.nullable D.string))



-- VIEW


view : Model -> Html Msg
view model =
    let
        season = seasonOf model.today
    in
    div [ A.class "app", A.style "background" (seasonBg season), A.style "min-height" "100vh" ]
        [ viewSeasonBanner model season
        , viewHeader model
        , case model.error of
            Just e -> div [ A.class "error" ] [ text e ]
            Nothing -> text ""
        , viewViewSwitch model
        , case model.viewMode of
            CoachView -> viewCoachPage model
            CalendarView -> viewCalendarPage model
            JournalView -> viewJournalPage model
            AlmanacView -> viewAlmanacPage model
        ]


viewSeasonBanner : Model -> Season -> Html Msg
viewSeasonBanner model _ =
    let
        viewDate = effectiveToday model
        viewSeason = seasonOf viewDate
        offset = viewOffset model
        activeCount =
            case model.calendar of
                Just c -> c.species |> List.filter (speciesActiveInSeason viewSeason) |> List.length
                Nothing -> 0
        offsetLabel =
            if offset == 0 then
                "aujourd'hui"
            else if offset > 0 then
                "J+" ++ String.fromInt offset
            else
                "J" ++ String.fromInt offset
    in
    div [ A.class "panel", A.style "padding" "0.6rem" ]
        [ div [ A.style "display" "flex", A.style "align-items" "center", A.style "gap" "0.6rem", A.style "margin-bottom" "0.3rem" ]
            [ span [ A.style "font-size" "1.5rem" ] [ text (seasonIcon viewSeason) ]
            , Html.strong [] [ text (seasonLabel viewSeason) ]
            , span [ A.style "color" "#8b6e3d" ] [ text "·" ]
            , Html.strong [] [ text viewDate ]
            , span [ A.style "color" "#8b6e3d", A.style "font-size" "0.82rem" ]
                [ text ("(" ++ offsetLabel ++ ")") ]
            , span [ A.style "margin-left" "auto", A.style "font-size" "0.82rem", A.style "color" "#5a3a22" ]
                [ text (String.fromInt activeCount ++ " espèces actives") ]
            ]
        , div [ A.style "display" "flex", A.style "align-items" "center", A.style "gap" "0.5rem" ]
            [ span [ A.style "font-size" "0.78rem", A.style "color" "#5a3a22", A.style "min-width" "60px" ]
                [ text "1 janv" ]
            , Html.input
                [ A.type_ "range"
                , A.min "1"
                , A.max "365"
                , A.value (String.fromInt model.viewDoy)
                , E.onInput (\s -> SetViewDoy (String.toInt s |> Maybe.withDefault model.viewDoy))
                , A.style "flex" "1"
                ]
                []
            , span [ A.style "font-size" "0.78rem", A.style "color" "#5a3a22", A.style "min-width" "60px", A.style "text-align" "right" ]
                [ text "31 déc" ]
            , button
                [ E.onClick ResetViewDoy
                , A.disabled (offset == 0)
                , A.style "font-size" "0.78rem"
                ]
                [ text "↺ aujourd'hui" ]
            , if List.isEmpty model.historical then
                button
                    [ E.onClick LoadHistorical, A.disabled model.historicalLoading
                    , A.style "font-size" "0.78rem"
                    , A.title "Charge la météo moyenne des 5 dernières années pour estimer le futur lointain"
                    ]
                    [ text (if model.historicalLoading then "⏳ ..." else "📊 météo 5 ans") ]
              else
                span [ A.style "font-size" "0.72rem", A.style "color" "#5a8a35", A.title "Météo moyenne 5 ans chargée" ]
                    [ text "✓ 5 ans" ]
            ]
        ]


viewHeader : Model -> Html Msg
viewHeader model =
    div [ A.class "header" ]
        [ h1 [] [ text "🌱 Hortus" ]
        , div [ A.class "meta" ]
            [ case model.calendar of
                Just c ->
                    span [] [ text ("📍 " ++ c.location.name ++ " · " ++ String.fromFloat (round1 c.location.altitudeM) ++ " m") ]

                Nothing ->
                    text "Chargement..."
            ]
        ]


viewViewSwitch : Model -> Html Msg
viewViewSwitch model =
    div [ A.class "panel", A.style "padding" "0.5rem" ]
        [ div [ A.class "controls" ]
            [ button
                [ E.onClick (SetViewMode CoachView)
                , A.classList [ ( "primary", model.viewMode == CoachView ) ]
                ]
                [ text "🎯 Coach" ]
            , button
                [ E.onClick (SetViewMode CalendarView)
                , A.classList [ ( "primary", model.viewMode == CalendarView ) ]
                ]
                [ text "📅 Calendrier" ]
            , button
                [ E.onClick (SetViewMode JournalView)
                , A.classList [ ( "primary", model.viewMode == JournalView ) ]
                ]
                [ text "📓 Mon jardin" ]
            , button
                [ E.onClick (SetViewMode AlmanacView)
                , A.classList [ ( "primary", model.viewMode == AlmanacView ) ]
                ]
                [ text "📖 Almanach" ]
            ]
        ]


viewCoachPage : Model -> Html Msg
viewCoachPage model =
    case model.calendar of
        Nothing ->
            div [ A.class "panel" ] [ text "Chargement du catalogue..." ]

        Just cal ->
            div [ A.class "layout" ]
                [ div []
                    [ viewCoachTodo model cal
                    , viewCoachWeek model
                    ]
                , div []
                    [ viewCoachWatch model cal
                    , viewCoachObservations model
                    , viewCoachTip model
                    ]
                ]


-- Panneau "Mes observations" (notes texte saisies sur les plants)


viewCoachObservations : Model -> Html Msg
viewCoachObservations model =
    let
        observations =
            observationNotes model
                |> List.sortBy .date
                |> List.reverse
                |> List.take 12
    in
    div [ A.class "panel" ]
        [ h2 [] [ text "📝 Mes observations" ]
        , if List.isEmpty observations then
            p [ A.style "color" "#5a3a22", A.style "font-size" "0.85rem" ]
                [ text "Aucune observation. Clique un plant → écris ce que tu constates." ]
          else
            div [ A.style "display" "flex", A.style "flex-direction" "column", A.style "gap" "0.4rem" ]
                (List.map viewObservationItem observations)
        ]


viewObservationItem : ActionEntry -> Html Msg
viewObservationItem a =
    let
        sid = a.speciesId |> Maybe.withDefault ""
        head =
            if sid == "" then
                a.date
            else
                speciesEmoji sid ++ " " ++ speciesShortName sid ++ " · " ++ a.date
    in
    div
        [ A.style "padding" "0.4rem 0.55rem", A.style "background" "#f5f0e0"
        , A.style "border-left" "3px solid #5a7a22", A.style "border-radius" "4px"
        ]
        [ div [ A.style "font-size" "0.74rem", A.style "color" "#5a3a22", A.style "font-weight" "600" ]
            [ text head ]
        , div [ A.style "font-size" "0.85rem", A.style "margin-top" "0.15rem" ]
            [ text (a.notes |> Maybe.withDefault "") ]
        , case a.solution of
            Just sol ->
                if String.trim sol == "" then
                    text ""
                else
                    div [ A.style "font-size" "0.8rem", A.style "margin-top" "0.15rem", A.style "color" "#3a6a1a" ]
                        [ text ("💡 " ++ sol) ]

            Nothing -> text ""
        ]


-- ALMANACH : historique des observations, solutions, recherche


-- Notes d'observation réelles (kind "note", texte non vide, hors marqueur
-- interne "lot" posé par les actions en lot).
observationNotes : Model -> List ActionEntry
observationNotes model =
    model.actions
        |> List.filter
            (\a ->
                a.kind == "note"
                    && (a.notes |> Maybe.map (\n -> String.trim n /= "" && n /= "lot") |> Maybe.withDefault False)
            )


problemCategories : List ( String, String )
problemCategories =
    [ ( "maladie", "🦠 Maladie" )
    , ( "ravageur", "🐛 Ravageur" )
    , ( "carence", "🍂 Carence" )
    , ( "climat", "⛈ Climat" )
    , ( "croissance", "📉 Croissance" )
    , ( "germination", "🌱 Germination" )
    , ( "autre", "❓ Autre" )
    ]


categoryLabel : String -> String
categoryLabel cat =
    problemCategories
        |> List.filter (\( k, _ ) -> k == cat)
        |> List.head
        |> Maybe.map Tuple.second
        |> Maybe.withDefault cat


entryKindIcon : String -> String
entryKindIcon kind =
    case kind of
        "observation" -> "👁"
        "traitement" -> "🧪"
        "resultat" -> "📊"
        _ -> "·"


viewAlmanacPage : Model -> Html Msg
viewAlmanacPage model =
    let
        q = String.toLower (String.trim model.almanacSearch)

        matchesNote a =
            q == ""
                || List.any (String.contains q)
                    [ String.toLower (a.notes |> Maybe.withDefault "")
                    , String.toLower (a.solution |> Maybe.withDefault "")
                    , String.toLower (a.speciesId |> Maybe.map speciesShortName |> Maybe.withDefault "")
                    , a.date
                    ]

        matchesProblem p =
            q == ""
                || List.any (String.contains q)
                    ([ String.toLower p.title
                     , String.toLower p.category
                     , String.toLower (p.speciesId |> Maybe.map speciesShortName |> Maybe.withDefault "")
                     , String.toLower (p.conclusion |> Maybe.withDefault "")
                     ]
                        ++ List.map (.text >> String.toLower) p.entries
                    )

        notes =
            observationNotes model
                |> List.filter matchesNote
                |> List.sortBy .date
                |> List.reverse

        probs = model.problems |> List.filter matchesProblem
        openProbs = probs |> List.filter (\p -> p.status /= "resolved")
        resolvedProbs = probs |> List.filter (\p -> p.status == "resolved")

        sectionTitle icon label count =
            h3 [ A.style "margin" "0 0 0.6rem 0" ]
                [ text (icon ++ " " ++ label ++ " (" ++ String.fromInt count ++ ")") ]
    in
    div []
        [ div [ A.class "panel" ]
            [ h2 [] [ text "📖 Almanach du jardin" ]
            , p [ A.style "font-size" "0.85rem", A.style "color" "#5a3a22" ]
                [ text "Suivi méthodique des problèmes : une fiche par souci, avec observations, traitements testés et résultats datés. Clos la fiche avec ta conclusion pour la retrouver les saisons suivantes." ]
            , div [ A.style "display" "flex", A.style "gap" "0.6rem", A.style "align-items" "center", A.style "flex-wrap" "wrap" ]
                [ button
                    [ E.onClick (OpenProblemForm Nothing Nothing)
                    , A.style "padding" "6px 12px", A.style "background" "#5a7a22"
                    , A.style "color" "white", A.style "border" "none"
                    , A.style "border-radius" "4px", A.style "cursor" "pointer"
                    , A.style "font-size" "0.85rem"
                    ]
                    [ text "🔬 Nouvelle fiche" ]
                , input
                    [ A.type_ "search"
                    , A.placeholder "🔍 Rechercher (espèce, symptôme, traitement…)"
                    , A.value model.almanacSearch
                    , E.onInput SetAlmanacSearch
                    , A.style "flex" "1", A.style "min-width" "220px"
                    , A.style "padding" "6px 10px", A.style "font-size" "0.85rem"
                    ]
                    []
                ]
            , case model.newProblem of
                Just draft -> viewNewProblemForm model draft
                Nothing -> text ""
            ]
        , div [ A.class "panel" ]
            [ sectionTitle "🔴" "Problèmes en cours" (List.length openProbs)
            , if List.isEmpty openProbs then
                p [ A.style "font-size" "0.84rem", A.style "color" "#5a3a22" ]
                    [ text "Aucun problème ouvert — le jardin se porte bien 🌞" ]
              else
                div [ A.style "display" "flex", A.style "flex-direction" "column", A.style "gap" "0.8rem" ]
                    (List.map (viewProblemCard model) openProbs)
            ]
        , div [ A.class "panel" ]
            [ sectionTitle "✅" "Problèmes résolus" (List.length resolvedProbs)
            , if List.isEmpty resolvedProbs then
                p [ A.style "font-size" "0.84rem", A.style "color" "#5a3a22" ]
                    [ text "Les fiches closes avec leur conclusion apparaîtront ici." ]
              else
                div [ A.style "display" "flex", A.style "flex-direction" "column", A.style "gap" "0.8rem" ]
                    (List.map (viewProblemCard model) resolvedProbs)
            ]
        , div [ A.class "panel" ]
            [ sectionTitle "📝" "Notes libres" (List.length notes)
            , if List.isEmpty notes then
                p [ A.style "font-size" "0.84rem", A.style "color" "#5a3a22" ]
                    [ text "Les observations rapides saisies sur un plant apparaissent ici." ]
              else
                div [ A.style "display" "flex", A.style "flex-direction" "column", A.style "gap" "1rem" ]
                    (groupNotesBySpecies notes |> List.map (viewAlmanacGroup model))
            ]
        ]


viewNewProblemForm : Model -> NewProblemDraft -> Html Msg
viewNewProblemForm model draft =
    let
        speciesOptions =
            case model.calendar of
                Just cal ->
                    cal.species
                        |> List.map (\sl -> sl.species.id)
                        |> List.map
                            (\sid ->
                                option [ A.value sid, A.selected (draft.speciesId == sid) ]
                                    [ text (speciesEmoji sid ++ " " ++ speciesShortName sid) ]
                            )

                Nothing -> []

        lbl t =
            div [ A.style "font-size" "0.76rem", A.style "color" "#5a3a22", A.style "font-weight" "600", A.style "margin-top" "0.5rem" ]
                [ text t ]
    in
    div
        [ A.style "margin-top" "0.7rem", A.style "padding" "0.7rem"
        , A.style "background" "#fff6de", A.style "border" "2px solid #d4a033"
        , A.style "border-radius" "6px"
        ]
        [ h3 [ A.style "margin" "0" ] [ text "🔬 Nouvelle fiche problème" ]
        , lbl "Espèce concernée"
        , select
            [ E.onInput SetProblemSpecies
            , A.style "width" "100%", A.style "padding" "5px", A.style "font-size" "0.85rem"
            ]
            (option [ A.value "", A.selected (draft.speciesId == "") ] [ text "— générale (tout le jardin) —" ]
                :: speciesOptions
            )
        , lbl "Titre du problème"
        , input
            [ A.type_ "text"
            , A.value draft.title
            , E.onInput SetProblemTitle
            , A.placeholder "Ex : feuilles jaunes en bas des plants"
            , A.style "width" "100%", A.style "padding" "5px 8px"
            , A.style "box-sizing" "border-box", A.style "font-size" "0.85rem"
            ]
            []
        , lbl "Catégorie"
        , select
            [ E.onInput SetProblemCategory
            , A.style "width" "100%", A.style "padding" "5px", A.style "font-size" "0.85rem"
            ]
            (problemCategories
                |> List.map
                    (\( k, label ) ->
                        option [ A.value k, A.selected (draft.category == k) ] [ text label ]
                    )
            )
        , lbl "Première observation (symptômes, hypothèse…)"
        , textarea
            [ A.value draft.firstObs
            , E.onInput SetProblemObs
            , A.placeholder "Ex : jaunissement des feuilles basses depuis 3 jours, taches brunes. Hypothèse : mildiou ou carence en magnésium."
            , A.rows 3
            , A.style "width" "100%", A.style "box-sizing" "border-box", A.style "font-size" "0.85rem"
            ]
            []
        , div [ A.style "margin-top" "0.5rem", A.style "display" "flex", A.style "gap" "0.4rem" ]
            [ button
                [ E.onClick SubmitProblem
                , A.style "padding" "6px 12px", A.style "background" "#5a7a22"
                , A.style "color" "white", A.style "border" "none"
                , A.style "border-radius" "4px", A.style "cursor" "pointer"
                , A.style "font-size" "0.85rem"
                ]
                [ text "💾 Créer la fiche" ]
            , button
                [ E.onClick CancelProblemForm, A.style "padding" "6px 12px", A.style "font-size" "0.85rem" ]
                [ text "Annuler" ]
            ]
        ]


viewProblemCard : Model -> Problem -> Html Msg
viewProblemCard model p =
    let
        resolved = p.status == "resolved"
        sid = p.speciesId |> Maybe.withDefault ""
        speciesLabel =
            if sid == "" then
                "🌍 Général"
            else
                speciesEmoji sid ++ " " ++ speciesShortName sid

        entryLine e =
            div [ A.style "display" "flex", A.style "gap" "0.5rem", A.style "font-size" "0.84rem", A.style "padding" "0.2rem 0" ]
                [ span [ A.style "color" "#5a3a22", A.style "font-size" "0.76rem", A.style "white-space" "nowrap", A.style "font-weight" "600" ]
                    [ text e.date ]
                , span [] [ text (entryKindIcon e.kind ++ " " ++ e.text) ]
                ]

        addEntryBtn kind label =
            button
                [ E.onClick (StartEntry p.id kind)
                , A.style "padding" "3px 9px", A.style "font-size" "0.76rem"
                , A.style "background" "#fff6de", A.style "border" "1px solid #d4a033"
                , A.style "border-radius" "4px", A.style "cursor" "pointer"
                ]
                [ text label ]

        entryEditor =
            case model.entryDraft of
                Just ed ->
                    if ed.problemId == p.id then
                        [ textarea
                            [ A.value ed.text
                            , E.onInput SetEntryText
                            , A.placeholder
                                (case ed.kind of
                                    "traitement" -> "Ex : purin d'ortie dilué à 10 %, pulvérisé le soir"
                                    "resultat" -> "Ex : jaunissement stoppé après 5 jours, taches stables"
                                    _ -> "Ex : les taches gagnent les feuilles du milieu"
                                )
                            , A.rows 2
                            , A.style "width" "100%", A.style "margin-top" "0.3rem"
                            , A.style "box-sizing" "border-box", A.style "font-size" "0.84rem"
                            ]
                            []
                        , div [ A.style "margin-top" "0.3rem", A.style "display" "flex", A.style "gap" "0.4rem" ]
                            [ button
                                [ E.onClick SubmitEntry
                                , A.style "padding" "4px 10px", A.style "background" "#5a7a22"
                                , A.style "color" "white", A.style "border" "none"
                                , A.style "border-radius" "4px", A.style "cursor" "pointer"
                                , A.style "font-size" "0.8rem"
                                ]
                                [ text ("💾 Ajouter " ++ entryKindIcon ed.kind) ]
                            , button [ E.onClick CancelEntry, A.style "font-size" "0.8rem" ] [ text "Annuler" ]
                            ]
                        ]
                    else
                        []

                Nothing -> []

        closeEditor =
            case model.closeDraft of
                Just ( pid, txt ) ->
                    if pid == p.id then
                        [ textarea
                            [ A.value txt
                            , E.onInput SetCloseText
                            , A.placeholder "Conclusion : qu'est-ce qui a marché, à refaire ou éviter l'an prochain ?"
                            , A.rows 2
                            , A.style "width" "100%", A.style "margin-top" "0.3rem"
                            , A.style "box-sizing" "border-box", A.style "font-size" "0.84rem"
                            ]
                            []
                        , div [ A.style "margin-top" "0.3rem", A.style "display" "flex", A.style "gap" "0.4rem" ]
                            [ button
                                [ E.onClick SubmitClose
                                , A.style "padding" "4px 10px", A.style "background" "#4a9b3c"
                                , A.style "color" "white", A.style "border" "none"
                                , A.style "border-radius" "4px", A.style "cursor" "pointer"
                                , A.style "font-size" "0.8rem"
                                ]
                                [ text "✅ Clore la fiche" ]
                            , button [ E.onClick CancelClose, A.style "font-size" "0.8rem" ] [ text "Annuler" ]
                            ]
                        ]
                    else
                        []

                Nothing -> []
    in
    div
        [ A.style "padding" "0.6rem 0.8rem"
        , A.style "background" (if resolved then "#eef5e0" else "#fdf3e3")
        , A.style "border-left" ("4px solid " ++ (if resolved then "#4a9b3c" else "#c0392b"))
        , A.style "border-radius" "5px"
        ]
        ([ div [ A.style "display" "flex", A.style "justify-content" "space-between", A.style "align-items" "baseline", A.style "flex-wrap" "wrap", A.style "gap" "0.3rem" ]
            [ div [ A.style "font-weight" "700", A.style "font-size" "0.95rem" ]
                [ text ((if resolved then "✅ " else "🔴 ") ++ p.title) ]
            , div [ A.style "font-size" "0.76rem", A.style "color" "#5a3a22" ]
                [ text (speciesLabel ++ " · " ++ categoryLabel p.category) ]
            ]
         , div [ A.style "margin-top" "0.4rem", A.style "border-left" "2px solid #d4b85a", A.style "padding-left" "0.6rem" ]
            (List.map entryLine p.entries)
         ]
            ++ (case p.conclusion of
                    Just c ->
                        if String.trim c == "" then
                            []
                        else
                            [ div
                                [ A.style "margin-top" "0.4rem", A.style "padding" "0.35rem 0.5rem"
                                , A.style "background" "#dcedc8", A.style "border-radius" "4px"
                                , A.style "font-size" "0.84rem", A.style "color" "#2a4a10"
                                ]
                                [ text ("💡 " ++ c) ]
                            ]

                    Nothing -> []
               )
            ++ entryEditor
            ++ closeEditor
            ++ [ div [ A.style "margin-top" "0.5rem", A.style "display" "flex", A.style "gap" "0.35rem", A.style "flex-wrap" "wrap" ]
                    (if resolved then
                        [ button
                            [ E.onClick (ReopenProblem p.id)
                            , A.style "padding" "3px 9px", A.style "font-size" "0.76rem", A.style "cursor" "pointer"
                            ]
                            [ text "🔄 Rouvrir" ]
                        , button
                            [ E.onClick (DeleteProblem p.id)
                            , A.style "padding" "3px 9px", A.style "font-size" "0.76rem"
                            , A.style "color" "#a03030", A.style "cursor" "pointer"
                            ]
                            [ text "🗑 Supprimer" ]
                        ]

                     else
                        [ addEntryBtn "observation" "👁 Observation"
                        , addEntryBtn "traitement" "🧪 Traitement"
                        , addEntryBtn "resultat" "📊 Résultat"
                        , button
                            [ E.onClick (StartClose p.id)
                            , A.style "padding" "3px 9px", A.style "font-size" "0.76rem"
                            , A.style "background" "#4a9b3c", A.style "color" "white"
                            , A.style "border" "none", A.style "border-radius" "4px"
                            , A.style "cursor" "pointer"
                            ]
                            [ text "✅ Clore" ]
                        , button
                            [ E.onClick (DeleteProblem p.id)
                            , A.style "padding" "3px 9px", A.style "font-size" "0.76rem"
                            , A.style "color" "#a03030", A.style "cursor" "pointer"
                            ]
                            [ text "🗑" ]
                        ]
                    )
               ]
        )


-- Groupe les notes par espèce, dans l'ordre de première apparition
-- (donc l'espèce avec la note la plus récente en premier).
groupNotesBySpecies : List ActionEntry -> List ( String, List ActionEntry )
groupNotesBySpecies notes =
    let
        keyOf a = a.speciesId |> Maybe.withDefault ""
        keys =
            List.foldl
                (\a acc ->
                    if List.member (keyOf a) acc then acc else acc ++ [ keyOf a ]
                )
                []
                notes
    in
    keys |> List.map (\k -> ( k, List.filter (\a -> keyOf a == k) notes ))


viewAlmanacGroup : Model -> ( String, List ActionEntry ) -> Html Msg
viewAlmanacGroup model ( sid, notes ) =
    let
        title =
            if sid == "" then
                "📌 Général"
            else
                speciesEmoji sid ++ " " ++ speciesShortName sid
    in
    div []
        [ h3 [ A.style "margin" "0 0 0.4rem 0", A.style "font-size" "1rem" ] [ text title ]
        , div [ A.style "display" "flex", A.style "flex-direction" "column", A.style "gap" "0.5rem" ]
            (List.map (viewAlmanacEntry model) notes)
        ]


viewAlmanacEntry : Model -> ActionEntry -> Html Msg
viewAlmanacEntry model a =
    let
        sol = a.solution |> Maybe.map String.trim |> Maybe.withDefault ""
        resolved = sol /= ""
        editing =
            case model.solutionDraft of
                Just ( nid, txt ) ->
                    if nid == a.id then Just txt else Nothing

                Nothing -> Nothing
    in
    div
        [ A.style "padding" "0.5rem 0.7rem"
        , A.style "background" (if resolved then "#eef5e0" else "#f5f0e0")
        , A.style "border-left" ("3px solid " ++ (if resolved then "#4a9b3c" else "#d4a033"))
        , A.style "border-radius" "4px"
        ]
        ([ div [ A.style "font-size" "0.74rem", A.style "color" "#5a3a22", A.style "font-weight" "600" ]
            [ text (a.date ++ (if resolved then " · ✅ résolu" else "")) ]
         , div [ A.style "font-size" "0.88rem", A.style "margin-top" "0.15rem" ]
            [ text (a.notes |> Maybe.withDefault "") ]
         ]
            ++ (case editing of
                    Just txt ->
                        [ textarea
                            [ A.value txt
                            , E.onInput SetSolutionDraft
                            , A.placeholder "Ex : ombrage + arrosage le soir, résolu en 1 semaine"
                            , A.rows 2
                            , A.style "width" "100%", A.style "margin-top" "0.35rem"
                            , A.style "box-sizing" "border-box", A.style "font-size" "0.84rem"
                            ]
                            []
                        , div [ A.style "margin-top" "0.3rem", A.style "display" "flex", A.style "gap" "0.4rem" ]
                            [ button
                                [ E.onClick (SaveSolution a.id)
                                , A.style "padding" "4px 10px", A.style "background" "#4a9b3c"
                                , A.style "color" "white", A.style "border" "none"
                                , A.style "border-radius" "4px", A.style "cursor" "pointer"
                                , A.style "font-size" "0.8rem"
                                ]
                                [ text "💾 Enregistrer" ]
                            , button
                                [ E.onClick CancelSolution
                                , A.style "padding" "4px 10px", A.style "font-size" "0.8rem"
                                ]
                                [ text "Annuler" ]
                            ]
                        ]

                    Nothing ->
                        if resolved then
                            [ div [ A.style "font-size" "0.84rem", A.style "margin-top" "0.3rem", A.style "color" "#3a6a1a" ]
                                [ text ("💡 " ++ sol)
                                , button
                                    [ E.onClick (EditSolution a.id sol)
                                    , A.style "margin-left" "0.5rem", A.style "font-size" "0.72rem"
                                    , A.style "cursor" "pointer"
                                    ]
                                    [ text "✏ modifier" ]
                                ]
                            ]
                        else
                            [ div [ A.style "margin-top" "0.3rem" ]
                                [ button
                                    [ E.onClick (EditSolution a.id "")
                                    , A.style "padding" "3px 9px", A.style "font-size" "0.78rem"
                                    , A.style "background" "#fff6de", A.style "border" "1px solid #d4a033"
                                    , A.style "border-radius" "4px", A.style "cursor" "pointer"
                                    ]
                                    [ text "💡 Ajouter la solution" ]
                                ]
                            ]
               )
        )


-- Panneau "À faire aujourd'hui"


viewCoachTodo : Model -> CalendarResponse -> Html Msg
viewCoachTodo model cal =
    let
        today = effectiveToday model
        doy = isoToDoy today
        alreadySownThisYear speciesId =
            model.actions
                |> List.any
                    (\a ->
                        a.speciesId == Just speciesId
                            && List.member a.kind [ "semis_direct", "semis_abri", "repiquage" ]
                            && sameYear a.date today
                    )

        sowSuggestions =
            cal.species
                |> List.filterMap
                    (\sl ->
                        let
                            openIndoor =
                                case sl.indoorSowLocal of
                                    Just w -> doyInWindow doy w
                                    Nothing -> False
                            openDirect =
                                case sl.directSowLocal of
                                    Just w -> doyInWindow doy w
                                    Nothing -> False
                        in
                        if (openIndoor || openDirect) && not (alreadySownThisYear sl.species.id) then
                            Just
                                { species = sl.species
                                , indoor = openIndoor
                                , direct = openDirect
                                , daysLeft =
                                    [ ( openIndoor, sl.indoorSowLocal )
                                    , ( openDirect, sl.directSowLocal )
                                    ]
                                        |> List.filterMap (\( open, mw ) -> if open then mw else Nothing)
                                        |> List.map (\w -> daysLeftInWindow doy w)
                                        |> List.minimum
                                        |> Maybe.withDefault 0
                                }
                        else
                            Nothing
                    )
                |> List.sortBy .daysLeft
                |> List.take 8

        terrainPlants = plantsFromActions model
        shelterPlants = shelterPlantsFromActions model
        allPlants = terrainPlants ++ shelterPlants

        harvestSuggestions =
            allPlants
                |> List.filterMap
                    (\pl ->
                        case pl.state of
                            TileMature sp -> Just ( pl, sp )
                            _ -> Nothing
                    )

        -- Pluie significative dans les 24h ?
        rainTomorrow =
            model.forecast
                |> List.head
                |> Maybe.map (\f -> f.precipitationMm >= 5.0)
                |> Maybe.withDefault False

        lastWaterDays plantId =
            model.actions
                |> List.filter (\a -> a.kind == "arrosage" && actionMatchesPlant a plantId allPlants)
                |> List.map (\a -> daysBetween a.date model.today)
                |> List.minimum
                |> Maybe.withDefault 999

        hasPaillage plant =
            model.actions
                |> List.any
                    (\a ->
                        a.kind == "paillage"
                            && a.gridX == Just plant.x
                            && a.gridY == Just plant.y
                    )

        waterSuggestions =
            if rainTomorrow then []
            else
                terrainPlants
                    |> List.filter
                        (\pl ->
                            case pl.state of
                                TileGrowing _ _ -> lastWaterDays pl.id > 4
                                TileSown _ -> lastWaterDays pl.id > 4
                                _ -> False
                        )

        mulchSuggestions =
            terrainPlants
                |> List.filter
                    (\pl ->
                        not (hasPaillage pl) && daysSinceSeed model pl > 14
                    )
    in
    div [ A.class "panel" ]
        [ h2 [] [ text "🎯 À faire aujourd'hui" ]
        , if List.isEmpty sowSuggestions && List.isEmpty harvestSuggestions && List.isEmpty waterSuggestions && List.isEmpty mulchSuggestions then
            p [ A.style "color" "#5a3a22", A.style "font-size" "0.88rem" ]
                [ text "Rien d'urgent. Profite ☕" ]
          else text ""
        , if not (List.isEmpty harvestSuggestions) then
            div [ A.style "margin-bottom" "0.6rem" ]
                [ h3 [] [ text "🌾 À récolter" ]
                , div [] (List.map viewHarvestSuggestion harvestSuggestions)
                ]
          else text ""
        , if not (List.isEmpty waterSuggestions) then
            div [ A.style "margin-bottom" "0.6rem" ]
                [ h3 [] [ text "💧 À arroser" ]
                , p [ A.style "font-size" "0.76rem", A.style "color" "#5a3a22", A.style "margin" "0 0 0.3rem 0" ]
                    [ text "Plus de 4 jours sans arrosage et pas de pluie >5 mm prévue demain." ]
                , div [] (List.map (viewMaintenanceSuggestion "arrosage" "💧") waterSuggestions)
                ]
          else text ""
        , if not (List.isEmpty mulchSuggestions) then
            div [ A.style "margin-bottom" "0.6rem" ]
                [ h3 [] [ text "🍂 À pailler" ]
                , p [ A.style "font-size" "0.76rem", A.style "color" "#5a3a22", A.style "margin" "0 0 0.3rem 0" ]
                    [ text "Plants installés depuis +14 jours sans paillage." ]
                , div [] (List.map (viewMaintenanceSuggestion "paillage" "🍂") mulchSuggestions)
                ]
          else text ""
        , if not (List.isEmpty sowSuggestions) then
            div []
                [ h3 [] [ text "🌱 À semer" ]
                , div [] (List.map viewSowSuggestion sowSuggestions)
                ]
          else text ""
        ]


actionMatchesPlant : ActionEntry -> Int -> List PlantOnTerrain -> Bool
actionMatchesPlant a plantId plants =
    case plants |> List.filter (\pl -> pl.id == plantId) |> List.head of
        Just plant ->
            a.gridX == Just plant.x && a.gridY == Just plant.y
        Nothing -> False


daysSinceSeed : Model -> PlantOnTerrain -> Int
daysSinceSeed model plant = daysBetween plant.date (effectiveToday model)


viewMaintenanceSuggestion : String -> String -> PlantOnTerrain -> Html Msg
viewMaintenanceSuggestion kind icon pl =
    let
        ( label, bgColor ) =
            case kind of
                "arrosage" -> ( "💧 Arroser", "#5a8ab8" )
                "paillage" -> ( "🍂 Pailler", "#8b6e3d" )
                _ -> ( "✓ Noter", "#5a3a22" )
    in
    div
        [ A.class "pantry-row"
        , A.style "padding" "0.4rem 0"
        , A.style "border-bottom" "1px dashed #e2d2a8"
        ]
        [ span []
            [ text (icon ++ " ")
            , text (speciesEmoji pl.speciesId ++ " ")
            , Html.strong [] [ text (speciesShortName pl.speciesId) ]
            , text (" (" ++ String.fromInt pl.x ++ "," ++ String.fromInt pl.y ++ ")")
            ]
        , button
            [ E.onClick (QuickAction pl.id kind)
            , A.style "padding" "4px 10px", A.style "font-size" "0.78rem"
            , A.style "background" bgColor, A.style "color" "white"
            , A.style "border" "none"
            , A.style "border-radius" "3px", A.style "cursor" "pointer"
            , A.style "font-weight" "600"
            ]
            [ text label ]
        ]


viewHarvestSuggestion : ( PlantOnTerrain, String ) -> Html Msg
viewHarvestSuggestion ( pl, sp ) =
    div
        [ A.class "pantry-row"
        , A.style "padding" "0.4rem 0"
        , A.style "border-bottom" "1px dashed #e2d2a8"
        ]
        [ span []
            [ text (speciesEmoji sp ++ " ")
            , Html.strong [] [ text (speciesShortName sp) ]
            , text (" (" ++ String.fromInt pl.x ++ "," ++ String.fromInt pl.y ++ ")")
            ]
        , button
            [ E.onClick (QuickAction pl.id "recolte")
            , A.style "padding" "3px 8px", A.style "font-size" "0.75rem"
            , A.style "background" "#d4a033", A.style "color" "white"
            , A.style "border" "none", A.style "border-radius" "3px", A.style "cursor" "pointer"
            ]
            [ text "🌾 récolter" ]
        ]


viewSowSuggestion : SowSuggest -> Html Msg
viewSowSuggestion s =
    let
        tags =
            (if s.indoor then [ "🌡 sous abri" ] else [])
                ++ (if s.direct then [ "🌱 pleine terre" ] else [])
    in
    div
        [ A.class "pantry-row"
        , A.style "padding" "0.4rem 0"
        , A.style "border-bottom" "1px dashed #e2d2a8"
        , A.style "flex-direction" "column"
        , A.style "align-items" "stretch"
        ]
        [ div [ A.style "display" "flex", A.style "justify-content" "space-between" ]
            [ span [] [ text (speciesEmoji s.species.id ++ " "), Html.strong [] [ text s.species.nameFr ] ]
            , span [ A.style "font-size" "0.78rem", A.style "color" "#8b6e3d" ]
                [ text
                    (if s.daysLeft > 0 then
                        "fenêtre " ++ String.fromInt s.daysLeft ++ " j restants"
                     else
                        "dernier jour !"
                    )
                ]
            ]
        , div [ A.style "font-size" "0.76rem", A.style "color" "#5a3a22" ]
            [ text (String.join " · " tags) ]
        ]


type alias SowSuggest =
    { species : Species
    , indoor : Bool
    , direct : Bool
    , daysLeft : Int
    }


daysLeftInWindow : Int -> CalendarWindow -> Int
daysLeftInWindow doy w =
    if w.doyStart <= w.doyEnd then
        if doy > w.doyEnd || doy < w.doyStart then 0
        else w.doyEnd - doy
    else
        -- wrap
        if doy >= w.doyStart then (365 - doy) + w.doyEnd
        else if doy <= w.doyEnd then w.doyEnd - doy
        else 0


isoToDoy : String -> Int
isoToDoy iso =
    case String.split "-" iso of
        [ _, m, d ] ->
            let
                month = String.toInt m |> Maybe.withDefault 1
                day = String.toInt d |> Maybe.withDefault 1
                base =
                    [ 0, 31, 59, 90, 120, 151, 181, 212, 243, 273, 304, 334 ]
                        |> List.drop (max 0 (month - 1))
                        |> List.head
                        |> Maybe.withDefault 0
            in
            base + day
        _ -> 1


sameYear : String -> String -> Bool
sameYear a b =
    String.left 4 a == String.left 4 b


-- Panneau "Ta semaine"


viewCoachWeek : Model -> Html Msg
viewCoachWeek model =
    let
        today = effectiveToday model
        weekActions =
            model.actions
                |> List.filter (\a -> daysBetween a.date today <= 7 && daysBetween a.date today >= 0)

        weekHarvestG =
            weekActions
                |> List.filter (\a -> a.kind == "recolte")
                |> List.filterMap .quantityG
                |> List.sum

        speciesThisYear =
            model.actions
                |> List.filter (\a -> sameYear a.date today)
                |> List.filterMap .speciesId
                |> uniqueStrings
                |> List.length

        activeParcels =
            model.parcels
                |> List.filter
                    (\p ->
                        case deriveState model p of
                            TileEmpty -> False
                            _ -> True
                    )
                |> List.length
    in
    div [ A.class "panel" ]
        [ h2 [] [ text "✅ Ta semaine" ]
        , div [ A.style "display" "grid", A.style "grid-template-columns" "auto auto", A.style "gap" "0.4rem 1rem", A.style "font-size" "0.9rem" ]
            [ span [ A.style "color" "#5a3a22" ] [ text "Actions notées (7 j)" ]
            , span [] [ Html.strong [] [ text (String.fromInt (List.length weekActions)) ] ]
            , span [ A.style "color" "#5a3a22" ] [ text "Récolté (7 j)" ]
            , span [] [ Html.strong [] [ text (formatQty weekHarvestG) ] ]
            , span [ A.style "color" "#5a3a22" ] [ text "Espèces cette année" ]
            , span [] [ Html.strong [] [ text (String.fromInt speciesThisYear) ] ]
            , span [ A.style "color" "#5a3a22" ] [ text "Parcelles actives" ]
            , span [] [ Html.strong [] [ text (String.fromInt activeParcels ++ " / " ++ String.fromInt (List.length model.parcels)) ] ]
            ]
        ]


uniqueStrings : List String -> List String
uniqueStrings xs =
    List.foldl (\s acc -> if List.member s acc then acc else s :: acc) [] xs


-- Panneau "À surveiller"


type WeatherAlert
    = FrostAlert Float -- tmin
    | HeatAlert Float -- tmax
    | StormAlert Float -- mm
    | HeavyRainAlert Float


frostSensitiveSpecies : List String
frostSensitiveSpecies =
    [ "tomato", "tomato_cherry", "pepper", "eggplant", "zucchini"
    , "butternut_squash", "pumpkin", "cucumber", "melon", "basil"
    ]


heatSensitiveSpecies : List String
heatSensitiveSpecies =
    [ "lettuce", "spinach", "arugula", "lambs_lettuce", "chervil", "coriander" ]


viewCoachWatch : Model -> CalendarResponse -> Html Msg
viewCoachWatch model _ =
    let
        viewDate = effectiveToday model
        offset = viewOffset model

        alertsByDay =
            if offset > 7 && not (List.isEmpty model.historical) then
                -- Mode "futur lointain" : utilise la moyenne 5 ans pour la date virtuelle
                let
                    doy = isoToDoy viewDate
                in
                model.historical
                    |> List.filter (\h -> h.doy >= doy && h.doy <= doy + 6)
                    |> List.filterMap
                        (\h ->
                            let
                                alerts =
                                    List.filterMap identity
                                        [ if h.tempMinC < 2 then Just (FrostAlert h.tempMinC) else Nothing
                                        , if h.tempMaxC >= 32 then Just (HeatAlert h.tempMaxC) else Nothing
                                        , if h.precipitationMm >= 8 then Just (HeavyRainAlert h.precipitationMm) else Nothing
                                        ]
                                dateStr = isoFromDoy doy h.doy viewDate
                            in
                            if List.isEmpty alerts then Nothing else Just ( dateStr ++ " (moyenne 5 ans)", alerts )
                        )
            else
                model.forecast
                    |> List.filterMap
                        (\f ->
                            let
                                alerts =
                                    List.filterMap identity
                                        [ if f.tempMinC < 0 then Just (FrostAlert f.tempMinC)
                                          else if f.tempMinC < 2 then Just (FrostAlert f.tempMinC)
                                          else Nothing
                                        , if f.tempMaxC >= 32 then Just (HeatAlert f.tempMaxC) else Nothing
                                        , if f.kind == "Storm" then Just (StormAlert f.precipitationMm) else Nothing
                                        , if f.precipitationMm >= 15 && f.kind /= "Storm" then Just (HeavyRainAlert f.precipitationMm) else Nothing
                                        ]
                            in
                            if List.isEmpty alerts then Nothing else Just ( f.date, alerts )
                        )

        activeParcelsBy =
            \species -> model.parcels
                |> List.filter
                    (\p ->
                        case deriveState model p of
                            TileGrowing sp _ -> List.member sp species
                            TileMature sp -> List.member sp species
                            TileSown sp -> List.member sp species
                            _ -> False
                    )
    in
    let
        title =
            if (viewOffset model) > 7 && not (List.isEmpty model.historical) then
                "⚠ À surveiller (moyenne 5 ans)"
            else
                "⚠ À surveiller (météo J+7)"
    in
    div [ A.class "panel" ]
        [ h2 [] [ text title ]
        , if (viewOffset model) > 7 && List.isEmpty model.historical then
            p [ A.style "color" "#8b6e3d", A.style "font-size" "0.82rem" ]
                [ text "Charge la météo 5 ans (📊 dans le bandeau) pour des alertes au-delà de J+7." ]
          else if List.isEmpty model.forecast && (viewOffset model) <= 7 then
            p [ A.style "color" "#8b6e3d", A.style "font-size" "0.82rem" ]
                [ text "Prévisions non chargées (réseau ?). Retente dans un moment." ]
          else if List.isEmpty alertsByDay then
            p [ A.style "color" "#5a3a22", A.style "font-size" "0.88rem" ]
                [ text "RAS sur 7 jours — pas de gel, canicule ou orage attendu." ]
          else
            div [] (List.map (viewDayAlerts model activeParcelsBy) alertsByDay)
        ]


viewDayAlerts : Model -> (List String -> List Parcel) -> ( String, List WeatherAlert ) -> Html Msg
viewDayAlerts model activeParcelsBy ( date, alerts ) =
    div
        [ A.style "padding" "0.5rem 0"
        , A.style "border-bottom" "1px dashed #e2d2a8"
        ]
        [ div [ A.style "font-size" "0.82rem", A.style "color" "#5a3a22", A.style "margin-bottom" "0.3rem" ]
            [ Html.strong [] [ text date ] ]
        , div [] (List.map (viewAlertDetail activeParcelsBy) alerts)
        ]


viewAlertDetail : (List String -> List Parcel) -> WeatherAlert -> Html Msg
viewAlertDetail activeParcelsBy alert =
    case alert of
        FrostAlert tmin ->
            let
                affected = activeParcelsBy frostSensitiveSpecies
                parcelText =
                    if List.isEmpty affected then ""
                    else " · protéger " ++ String.join ", " (List.map .name affected)
            in
            div [ A.style "color" (if tmin < 0 then "#a03030" else "#c06020"), A.style "font-size" "0.88rem" ]
                [ text ("❄ Gel Tmin " ++ String.fromFloat (round1 tmin) ++ "°C — voile d'hivernage sur cultures fragiles" ++ parcelText) ]

        HeatAlert tmax ->
            let
                affected = activeParcelsBy heatSensitiveSpecies
                parcelText =
                    if List.isEmpty affected then ""
                    else " · ombrer / arroser " ++ String.join ", " (List.map .name affected)
            in
            div [ A.style "color" "#c06020", A.style "font-size" "0.88rem" ]
                [ text ("🥵 Canicule Tmax " ++ String.fromFloat (round1 tmax) ++ "°C — paillage, arrosage matin tôt" ++ parcelText) ]

        StormAlert mm ->
            div [ A.style "color" "#a03030", A.style "font-size" "0.88rem" ]
                [ text ("⛈ Orage " ++ String.fromFloat (round1 mm) ++ " mm — récolter ce qui est prêt, butter les plants fragiles") ]

        HeavyRainAlert mm ->
            div [ A.style "color" "#5a8ab8", A.style "font-size" "0.88rem" ]
                [ text ("🌧 Forte pluie " ++ String.fromFloat (round1 mm) ++ " mm — pas d'arrosage nécessaire") ]


-- Panneau "Conseil de la semaine"


viewCoachTip : Model -> Html Msg
viewCoachTip model =
    let
        today = effectiveToday model
        season = seasonOf today
        tip = seasonalTip season (isoToDoy today)
    in
    div [ A.class "panel" ]
        [ h2 [] [ text "🎓 Conseil" ]
        , p [ A.style "font-size" "0.92rem", A.style "line-height" "1.5" ] [ text tip ]
        ]


seasonalTip : Season -> Int -> String
seasonalTip season doy =
    case season of
        Spring ->
            if doy < 75 then
                "Début de saison : démarre tes semis sous abri (tomates, poivrons, aubergines). Prépare tes planches, corrige le pH si besoin."
            else if doy < 110 then
                "Plein printemps : semis directs possibles (carottes, épinards, radis, pois). Attention aux gelées tardives jusqu'aux Saints de Glace (11-13 mai)."
            else
                "Après les Saints de Glace : sortie des plants frileux (tomates, courgettes, basilic). Paille pour conserver l'humidité dès l'installation."

        Summer ->
            if doy < 200 then
                "Début d'été : arrosage matin ou soir, jamais au soleil. Récoltes hâtives : radis, salades, petit pois. Pense à pailler copieusement."
            else if doy < 230 then
                "Cœur d'été : monte à graines des légumes-feuilles — semer mâche et chicorée en fin de mois pour l'automne. Surveille l'oïdium sur courgettes."
            else
                "Fin d'été : récoltes abondantes (tomates, courgettes, haricots). Commence les semis d'automne (mâche, épinard, navet)."

        Autumn ->
            if doy < 280 then
                "Début automne : installation des cultures d'hiver (poireaux, mâche). Ramasse les courges avant les premières gelées. Plant des fraisiers en place."
            else
                "Automne avancé : plantation d'ail et d'échalotes. Semis des fèves. Paillis épais sur les parcelles nues pour l'hiver."

        Winter ->
            if doy > 334 || doy < 32 then
                "Hiver : planification de la saison. Commande des graines. Taille des arbres fruitiers à pépins (pommier, poirier) par temps sec. Nourris les oiseaux."
            else
                "Fin d'hiver : derniers jours pour tailler. Démarre les premiers semis sous abri chauffé (tomates, poivrons, aubergines). Prépare les châssis."


viewCalendarPage : Model -> Html Msg
viewCalendarPage model =
    div []
        [ viewControls model
        , case model.calendar of
            Nothing ->
                if model.loading then
                    p [] [ text "Chargement..." ]
                else
                    p [] [ text "—" ]

            Just cal ->
                div []
                    [ viewCalendarInfo cal
                    , viewSeasonalSpecies model cal
                    , viewCalendar model cal
                    , viewSelectedSpecies model cal
                    ]
        ]


viewSeasonalSpecies : Model -> CalendarResponse -> Html Msg
viewSeasonalSpecies model cal =
    div [ A.class "panel" ]
        [ h2 [] [ text "Par saison" ]
        , div [ A.style "display" "grid", A.style "grid-template-columns" "repeat(4, 1fr)", A.style "gap" "0.6rem" ]
            (List.map (viewSeasonColumn model cal) [ Spring, Summer, Autumn, Winter ])
        ]


viewSeasonColumn : Model -> CalendarResponse -> Season -> Html Msg
viewSeasonColumn model cal season =
    let
        isCurrent = seasonOf model.today == season
        list =
            cal.species
                |> List.filter (speciesActiveInSeason season)
                |> List.take 20
    in
    div
        [ A.style "padding" "0.5rem"
        , A.style "border-radius" "5px"
        , A.style "background" (if isCurrent then "#fff6de" else "#f5ecd6aa")
        , A.style "border" (if isCurrent then "2px solid #d4a033" else "1px solid #d4b85a")
        ]
        [ div [ A.style "font-weight" "bold", A.style "margin-bottom" "0.3rem" ]
            [ text (seasonIcon season ++ " " ++ seasonLabel season) ]
        , div [ A.style "font-size" "0.78rem", A.style "line-height" "1.6" ]
            (List.map
                (\sl ->
                    div
                        [ E.onClick (SelectSpeciesRow sl.species.id)
                        , A.style "cursor" "pointer"
                        , A.style "padding" "1px 0"
                        ]
                        [ text (speciesEmoji sl.species.id ++ " " ++ sl.species.nameFr) ]
                )
                list
            )
        ]


viewControls : Model -> Html Msg
viewControls model =
    div [ A.class "panel" ]
        [ div [ A.style "display" "flex", A.style "gap" "0.6rem", A.style "flex-wrap" "wrap", A.style "align-items" "flex-end" ]
            [ div [ A.style "display" "flex", A.style "flex-direction" "column" ]
                [ Html.label [ A.style "font-size" "0.78rem", A.style "color" "#5a3a22" ] [ text "Ville" ]
                , select
                    [ E.onInput SetCity, A.style "padding" "4px"
                    , A.style "border" "1px solid #d4b85a", A.style "border-radius" "3px"
                    , A.style "background" "#fff6de"
                    ]
                    (List.map (\c -> option [ A.value c.slug, A.selected (c.slug == model.selectedCity) ] [ text c.name ]) model.cities)
                ]
            , div [ A.style "display" "flex", A.style "flex-direction" "column" ]
                [ Html.label [ A.style "font-size" "0.78rem", A.style "color" "#5a3a22" ] [ text "Catégorie" ]
                , select
                    [ E.onInput SetFilterCategory, A.style "padding" "4px"
                    , A.style "border" "1px solid #d4b85a", A.style "border-radius" "3px"
                    , A.style "background" "#fff6de"
                    ]
                    (List.map (categoryOption model.filterCategory) categoryOptions)
                ]
            , div [ A.style "display" "flex", A.style "flex-direction" "column" ]
                [ Html.label [ A.style "font-size" "0.78rem", A.style "color" "#5a3a22" ] [ text "Difficulté" ]
                , select
                    [ E.onInput SetFilterDifficulty, A.style "padding" "4px"
                    , A.style "border" "1px solid #d4b85a", A.style "border-radius" "3px"
                    , A.style "background" "#fff6de"
                    ]
                    [ option [ A.value "", A.selected (model.filterDifficulty == Nothing) ] [ text "(toutes)" ]
                    , option [ A.value "Beginner", A.selected (model.filterDifficulty == Just "Beginner") ] [ text "Débutant" ]
                    , option [ A.value "Intermediate", A.selected (model.filterDifficulty == Just "Intermediate") ] [ text "Intermédiaire" ]
                    , option [ A.value "Advanced", A.selected (model.filterDifficulty == Just "Advanced") ] [ text "Avancé" ]
                    ]
                ]
            , div [ A.style "display" "flex", A.style "flex-direction" "column", A.style "flex" "1 1 150px" ]
                [ Html.label [ A.style "font-size" "0.78rem", A.style "color" "#5a3a22" ] [ text "Rechercher" ]
                , input
                    [ A.type_ "text", A.value model.search, E.onInput SetSearch
                    , A.placeholder "nom, latin..."
                    , A.style "padding" "4px", A.style "border" "1px solid #d4b85a"
                    , A.style "border-radius" "3px", A.style "background" "#fff6de"
                    ]
                    []
                ]
            , button
                [ E.onClick RefreshClimate, A.disabled model.refreshingClimate
                , A.title "Télécharge 5 ans de météo Open-Meteo et recalcule les fenêtres locales"
                ]
                [ text (if model.refreshingClimate then "⏳ calcul..." else "🔄 climat local (5 ans)") ]
            ]
        ]


categoryOptions : List ( String, String )
categoryOptions =
    [ ( "", "(toutes)" )
    , ( "FruitVegetable", "Légume-fruit" )
    , ( "LeafyVegetable", "Légume-feuille" )
    , ( "RootVegetable", "Racine" )
    , ( "Legume", "Légumineuse" )
    , ( "Herb", "Aromate" )
    , ( "Berry", "Petit fruit" )
    , ( "TreeFruit", "Fruitier" )
    , ( "Nut", "Fruit à coque" )
    ]


categoryOption : Maybe String -> ( String, String ) -> Html Msg
categoryOption current ( value, label ) =
    option [ A.value value, A.selected (current == toMaybe value) ] [ text label ]


viewCalendarInfo : CalendarResponse -> Html Msg
viewCalendarInfo cal =
    div [ A.class "panel", A.style "font-size" "0.82rem", A.style "color" "#5a3a22" ]
        [ text ("Climat : " ++ cal.climateSource ++ " · " ++ String.fromInt (List.length cal.species) ++ " espèces") ]


viewCalendar : Model -> CalendarResponse -> Html Msg
viewCalendar model cal =
    let
        filtered = filterSpecies model cal.species
        labelWidth = 180
        barWidth = 820
        rowHeight = 28
        totalH = List.length filtered * rowHeight + 40
        totalW = labelWidth + barWidth
    in
    div [ A.class "panel" ]
        [ h2 [] [ text "Calendrier annuel" ]
        , p [ A.style "font-size" "0.78rem", A.style "color" "#5a3a22" ]
            [ text "🟠 Semis sous abri · 🟢 Semis direct · 🔵 Repiquage · 🟡 Récolte" ]
        , Svg.svg
            [ SA.viewBox ("0 0 " ++ String.fromInt totalW ++ " " ++ String.fromInt totalH)
            , SA.width (String.fromInt totalW)
            , SA.style "max-width:100%;height:auto;background:#f5ecd6;border:1px solid #d4b85a;border-radius:4px"
            ]
            (monthsHeader labelWidth barWidth
                ++ List.indexedMap (viewCalendarRow model labelWidth barWidth rowHeight) filtered
            )
        ]


filterSpecies : Model -> List SpeciesLocal -> List SpeciesLocal
filterSpecies model list =
    list
        |> List.filter
            (\sl ->
                (case model.filterCategory of
                    Just c -> sl.species.category == c
                    Nothing -> True
                )
                    && (case model.filterDifficulty of
                            Just d -> sl.species.difficulty == d
                            Nothing -> True
                       )
                    && (model.search == ""
                            || String.contains (String.toLower model.search) (String.toLower sl.species.nameFr)
                            || String.contains (String.toLower model.search) (String.toLower sl.species.nameLatin)
                       )
            )


monthsHeader : Int -> Int -> List (Svg.Svg Msg)
monthsHeader labelX barWidth =
    let months = [ "J", "F", "M", "A", "M", "J", "J", "A", "S", "O", "N", "D" ] in
    List.indexedMap
        (\i lbl ->
            let x = labelX + round (toFloat barWidth * toFloat i / 12.0) in
            Svg.text_
                [ SA.x (String.fromInt (x + 6)), SA.y "18", SA.fontSize "11"
                , SA.fill "#5a3a22", SA.fontFamily "monospace"
                ]
                [ Svg.text lbl ]
        )
        months


viewCalendarRow : Model -> Int -> Int -> Int -> Int -> SpeciesLocal -> Svg.Svg Msg
viewCalendarRow model labelX barWidth rowHeight i sl =
    let
        sp = sl.species
        y = 30 + i * rowHeight
        barY = y + 4
        barH = rowHeight - 10
        isSelected = model.selectedSpecies == Just sp.id
        mkBand color mw =
            case mw of
                Nothing -> []
                Just w -> windowBands labelX barWidth barY barH color w
        indoorBands = mkBand "#c66339" sl.indoorSowLocal
        directBands = mkBand "#6b9c47" sl.directSowLocal
        transplantBands = mkBand "#5a8ab8" sl.transplantLocal
        harvestBands = windowBands labelX barWidth barY barH "#d4a033" sl.harvestLocal
    in
    Svg.g [ SE.onClick (SelectSpeciesRow sp.id), SA.style "cursor:pointer" ]
        ([ Svg.rect
            [ SA.x "0", SA.y (String.fromInt (y + 1)), SA.width (String.fromInt (labelX + barWidth))
            , SA.height (String.fromInt (rowHeight - 2))
            , SA.fill (if isSelected then "#fff0c8" else "transparent")
            , SA.stroke (if isSelected then "#d4b85a" else "none")
            ]
            []
         , Svg.text_
            [ SA.x "6", SA.y (String.fromInt (barY + barH - 4)), SA.fontSize "11", SA.fill "#3d2818" ]
            [ Svg.text sp.nameFr ]
         , Svg.rect
            [ SA.x (String.fromInt labelX), SA.y (String.fromInt barY)
            , SA.width (String.fromInt barWidth), SA.height (String.fromInt barH)
            , SA.fill "#fff6de", SA.stroke "#e2d2a8"
            ]
            []
         ]
            ++ indoorBands
            ++ directBands
            ++ transplantBands
            ++ harvestBands
        )


windowBands : Int -> Int -> Int -> Int -> String -> CalendarWindow -> List (Svg.Svg Msg)
windowBands labelX barWidth barY barH color w =
    let
        x1 = labelX + round (toFloat barWidth * toFloat w.doyStart / 365.0)
        x2 = labelX + round (toFloat barWidth * toFloat w.doyEnd / 365.0)
    in
    if w.doyStart <= w.doyEnd then
        [ bandRect x1 barY (max 2 (x2 - x1)) barH color ]
    else
        let
            xEnd = labelX + barWidth
            xStart = labelX
        in
        [ bandRect x1 barY (max 2 (xEnd - x1)) barH color
        , bandRect xStart barY (max 2 (x2 - xStart)) barH color
        ]


bandRect : Int -> Int -> Int -> Int -> String -> Svg.Svg Msg
bandRect x y w h color =
    Svg.rect
        [ SA.x (String.fromInt x), SA.y (String.fromInt y)
        , SA.width (String.fromInt w), SA.height (String.fromInt h)
        , SA.fill color, SA.opacity "0.78"
        ]
        []


viewSelectedSpecies : Model -> CalendarResponse -> Html Msg
viewSelectedSpecies model cal =
    case model.selectedSpecies of
        Nothing -> text ""
        Just id ->
            case cal.species |> List.filter (\s -> s.species.id == id) |> List.head of
                Nothing -> text ""
                Just sl ->
                    let sp = sl.species in
                    div [ A.class "panel" ]
                        [ h2 [] [ text sp.nameFr ]
                        , p [ A.style "font-style" "italic", A.style "color" "#5a3a22" ] [ text sp.nameLatin ]
                        , div [ A.style "display" "grid", A.style "grid-template-columns" "auto 1fr", A.style "gap" "0.3rem 0.8rem", A.style "font-size" "0.85rem" ]
                            [ detailRow "Famille" sp.family
                            , detailRow "Cycle" sp.lifeCycle
                            , detailRow "Catégorie" sp.category
                            , detailRow "Difficulté" sp.difficulty
                            , detailRow "Profondeur semis" (String.fromFloat sp.depthCm ++ " cm")
                            , detailRow "Espacement" (String.fromInt sp.spacingCm ++ " cm")
                            , detailRow "Semis → récolte" (String.fromInt sp.daysToHarvest ++ " jours")
                            ]
                        , if not (List.isEmpty sp.notes) then
                            div [ A.style "margin-top" "0.8rem" ]
                                [ h3 [] [ text "Notes" ]
                                , div [] (List.map (\n -> p [ A.style "margin" "0.2rem 0" ] [ text ("• " ++ n) ]) sp.notes)
                                ]
                          else text ""
                        ]


detailRow : String -> String -> Html Msg
detailRow label value =
    Html.node "fragment" []
        [ span [ A.style "color" "#5a3a22" ] [ text label ]
        , span [] [ text value ]
        ]



-- JOURNAL VIEW


viewJournalPage : Model -> Html Msg
viewJournalPage model =
    let
        coachPanels =
            case model.calendar of
                Just cal ->
                    div
                        [ A.style "display" "grid"
                        , A.style "grid-template-columns" "repeat(auto-fit, minmax(280px, 1fr))"
                        , A.style "gap" "0.6rem"
                        , A.style "margin-bottom" "0.6rem"
                        ]
                        [ viewCoachTodo model cal
                        , viewCoachWatch model cal
                        , viewCoachWeek model
                        , viewCoachTip model
                        ]
                Nothing ->
                    text ""
    in
    div []
        [ coachPanels
        , div [ A.class "layout" ]
            [ div []
                [ viewShelter model
                , viewTerrain model
                , viewPlantContextMenu model
                , viewActionsTimeline model
                ]
            , div []
                [ viewPalette model
                , viewBulkPanel model
                ]
            ]
        ]


viewPlantContextMenu : Model -> Html Msg
viewPlantContextMenu model =
    case model.plantMenu of
        Nothing -> text ""
        Just id ->
            case model.actions |> List.filter (\a -> a.id == id) |> List.head of
                Nothing -> text ""
                Just a ->
                    let
                        sid = a.speciesId |> Maybe.withDefault "—"
                        btn kind icon label =
                            button
                                [ E.onClick (QuickAction id kind)
                                , A.style "margin" "0.2rem", A.style "padding" "6px 10px"
                                , A.style "background" (kindColor kind), A.style "color" "white"
                                , A.style "border" "none", A.style "border-radius" "4px"
                                , A.style "cursor" "pointer", A.style "font-size" "0.85rem"
                                ]
                                [ text (icon ++ " " ++ label) ]
                    in
                    div [ A.class "panel", A.style "background" "#fff6de", A.style "border" "2px solid #d4a033" ]
                        [ h3 [] [ text ("Actions rapides · " ++ speciesEmoji sid ++ " " ++ speciesShortName sid) ]
                        , p [ A.style "font-size" "0.78rem", A.style "color" "#5a3a22" ]
                            [ text ("Semé le " ++ a.date ++ " — enregistrement daté d'aujourd'hui (" ++ model.today ++ ")") ]
                        , div [ A.style "display" "flex", A.style "flex-wrap" "wrap" ]
                            [ btn "arrosage" "💧" "Arroser"
                            , btn "paillage" "🍂" "Pailler"
                            , btn "compost" "🌱" "Compost"
                            , btn "traitement" "💊" "Traiter"
                            , btn "recolte" "🌾" "Récolter"
                            , btn "arrachage" "🗑" "Arracher"
                            ]
                        , div [ A.style "margin-top" "0.6rem" ]
                            [ div [ A.style "font-size" "0.78rem", A.style "color" "#5a3a22", A.style "font-weight" "600" ]
                                [ text "📝 Observation" ]
                            , textarea
                                [ A.value model.noteDraft
                                , E.onInput SetNoteDraft
                                , A.placeholder "Ex : romanesco file en fleur, pas grossi…"
                                , A.rows 2
                                , A.style "width" "100%", A.style "margin-top" "0.2rem"
                                , A.style "box-sizing" "border-box", A.style "font-size" "0.82rem"
                                ]
                                []
                            , button
                                [ E.onClick (SaveObservation id)
                                , A.style "margin-top" "0.3rem", A.style "padding" "6px 10px"
                                , A.style "background" "#5a7a22", A.style "color" "white"
                                , A.style "border" "none", A.style "border-radius" "4px"
                                , A.style "cursor" "pointer", A.style "font-size" "0.85rem"
                                ]
                                [ text "💾 Enregistrer observation" ]
                            ]
                        , div [ A.style "margin-top" "0.5rem" ]
                            [ button
                                [ E.onClick (OpenProblemForm a.speciesId (Just id))
                                , A.style "padding" "5px 10px", A.style "background" "#c0392b"
                                , A.style "color" "white", A.style "border" "none"
                                , A.style "border-radius" "4px", A.style "cursor" "pointer"
                                , A.style "font-size" "0.8rem"
                                ]
                                [ text "🔬 Signaler un problème (fiche suivie)" ]
                            ]
                        , viewPastNotesForSpecies model sid
                        , div [ A.style "margin-top" "0.5rem" ]
                            [ button [ E.onClick ClosePlantMenu, A.style "font-size" "0.78rem" ] [ text "Fermer" ] ]
                        ]


-- Rappel almanach : fiches problèmes et observations passées de l'espèce.
viewPastNotesForSpecies : Model -> String -> Html Msg
viewPastNotesForSpecies model sid =
    let
        pastProblems =
            model.problems
                |> List.filter (\p -> p.speciesId == Just sid)
                |> List.take 3

        problemLine p =
            div [ A.style "font-size" "0.78rem", A.style "margin-top" "0.25rem", A.style "padding-left" "0.4rem", A.style "border-left" ("2px solid " ++ (if p.status == "resolved" then "#4a9b3c" else "#c0392b")) ]
                [ div [] [ text ((if p.status == "resolved" then "✅ " else "🔴 ") ++ p.title) ]
                , case p.conclusion |> Maybe.map String.trim of
                    Just c ->
                        if c == "" then text "" else div [ A.style "color" "#3a6a1a" ] [ text ("💡 " ++ c) ]

                    Nothing -> text ""
                ]

        problemsBlock =
            if List.isEmpty pastProblems then
                []
            else
                div [ A.style "font-size" "0.78rem", A.style "color" "#5a3a22", A.style "font-weight" "600", A.style "margin-top" "0.6rem" ]
                    [ text "🔬 Fiches problème de cette espèce" ]
                    :: List.map problemLine pastProblems

        past =
            observationNotes model
                |> List.filter (\a -> a.speciesId == Just sid)
                |> List.sortBy .date
                |> List.reverse
                |> List.take 3
    in
    if List.isEmpty past && List.isEmpty problemsBlock then
        text ""
    else if List.isEmpty past then
        div [] problemsBlock
    else
        div []
            (problemsBlock ++ [ viewPastNotesOnly past ])


viewPastNotesOnly : List ActionEntry -> Html Msg
viewPastNotesOnly past =
        div [ A.style "margin-top" "0.6rem" ]
            (div [ A.style "font-size" "0.78rem", A.style "color" "#5a3a22", A.style "font-weight" "600" ]
                [ text "📖 Déjà noté sur cette espèce" ]
                :: List.map
                    (\a ->
                        div [ A.style "font-size" "0.78rem", A.style "margin-top" "0.25rem", A.style "padding-left" "0.4rem", A.style "border-left" "2px solid #d4a033" ]
                            [ div [] [ text (a.date ++ " — " ++ (a.notes |> Maybe.withDefault "")) ]
                            , case a.solution |> Maybe.map String.trim of
                                Just sol ->
                                    if sol == "" then
                                        text ""
                                    else
                                        div [ A.style "color" "#3a6a1a" ] [ text ("💡 " ++ sol) ]

                                Nothing -> text ""
                            ]
                    )
                    past
            )


viewBulkPanel : Model -> Html Msg
viewBulkPanel model =
    let
        selected = bulkSelection model
        speciesList =
            case model.calendar of
                Just cal -> cal.species |> List.map (\sl -> ( sl.species.id, sl.species.nameFr ))
                Nothing -> []
    in
    div [ A.class "panel" ]
        [ h2 [] [ text "🎛 Actions en lot" ]
        , p [ A.style "font-size" "0.78rem", A.style "color" "#5a3a22" ]
            [ text "Applique une action à tous les plants correspondant au filtre." ]
        , div [ A.style "display" "flex", A.style "flex-direction" "column", A.style "gap" "0.4rem" ]
            [ labeledSelect "Action"
                model.bulkKind
                SetBulkKind
                [ ( "arrosage", "💧 Arroser" )
                , ( "paillage", "🍂 Pailler" )
                , ( "compost", "🌱 Compost" )
                , ( "traitement", "💊 Traiter" )
                , ( "recolte", "🌾 Récolter" )
                , ( "note", "📝 Note" )
                ]
            , labeledSelect "Zone"
                (case model.bulkZone of
                    Just Shelter -> "shelter"
                    Just Terrain -> "terrain"
                    Nothing -> ""
                )
                SetBulkZone
                [ ( "", "Toutes" ), ( "shelter", "🌡 Abri uniquement" ), ( "terrain", "🌱 Terrain uniquement" ) ]
            , labeledSelect "Espèce"
                (model.bulkSpeciesId |> Maybe.withDefault "")
                SetBulkSpecies
                (( "", "Toutes" ) :: speciesList)
            , Html.label [ A.style "font-size" "0.82rem", A.style "cursor" "pointer" ]
                [ Html.input
                    [ A.type_ "checkbox"
                    , A.checked model.bulkOnlyMature
                    , E.onClick ToggleBulkMature
                    , A.style "margin-right" "0.3rem"
                    ]
                    []
                , text "Matures uniquement"
                ]
            , div [ A.style "padding" "0.4rem", A.style "background" "#fff6de", A.style "border-radius" "4px", A.style "font-size" "0.82rem" ]
                [ Html.strong [] [ text (String.fromInt (List.length selected)) ]
                , text " plant(s) sélectionné(s)"
                ]
            , button
                [ E.onClick ApplyBulk
                , A.class "primary"
                , A.disabled (List.isEmpty selected)
                ]
                [ text ("✓ Appliquer sur " ++ String.fromInt (List.length selected) ++ " plant(s)") ]
            ]
        , Html.hr [ A.style "margin" "0.8rem 0", A.style "border" "0", A.style "border-top" "1px solid #d4b85a" ] []
        , h3 [ A.style "color" "#a03030" ] [ text "⚠ Zone dangereuse" ]
        , if model.confirmingClearAll then
            div [ A.style "padding" "0.5rem", A.style "background" "#fde8e8", A.style "border" "2px solid #a03030", A.style "border-radius" "4px" ]
                [ p [ A.style "margin" "0 0 0.4rem 0", A.style "font-size" "0.85rem" ]
                    [ text ("Effacer "
                        ++ String.fromInt (List.length model.actions)
                        ++ " action(s) ? Tous les plants disparaissent."
                        )
                    ]
                , div [ A.style "display" "flex", A.style "gap" "0.4rem" ]
                    [ button
                        [ E.onClick ConfirmClearAll, A.class "danger"
                        , A.style "background" "#a03030", A.style "color" "white"
                        , A.style "padding" "5px 10px", A.style "border" "none", A.style "border-radius" "4px"
                        , A.style "cursor" "pointer"
                        ]
                        [ text "🗑 Oui, tout effacer" ]
                    , button [ E.onClick CancelClearAll ] [ text "Annuler" ]
                    ]
                ]
          else
            button
                [ E.onClick RequestClearAll
                , A.disabled (List.isEmpty model.actions)
                , A.style "background" "#fff6de", A.style "border" "1px solid #a03030"
                , A.style "color" "#a03030", A.style "padding" "5px 10px"
                , A.style "border-radius" "4px", A.style "cursor" "pointer"
                , A.style "font-size" "0.85rem"
                ]
                [ text ("🗑 Tout effacer (" ++ String.fromInt (List.length model.actions) ++ ")") ]
        ]


viewShelter : Model -> Html Msg
viewShelter model =
    let
        w = 800
        h = 150
        widthM = toFloat w / 100
        heightM = toFloat h / 100
        areaM2 = round1 (widthM * heightM)
        hint =
            case model.paletteSpecies of
                Just sid -> "🌡 Clique dans l'abri pour mettre " ++ sid ++ " en pépinière."
                Nothing -> "Pépinière : sème sous abri, puis glisse-dépose vers le jardin pour repiquer."
        plants = shelterPlantsFromActions model
    in
    div [ A.class "panel" ]
        [ h3 [] [ text ("🌡 Abri / pépinière · " ++ String.fromFloat areaM2 ++ " m²") ]
        , p [ A.style "font-size" "0.76rem", A.style "color" "#5a3a22", A.style "margin" "0 0 0.3rem 0" ]
            [ text hint ]
        , Svg.svg
            [ SA.viewBox ("0 0 " ++ String.fromInt w ++ " " ++ String.fromInt h)
            , SA.width (String.fromInt w)
            , SE.on "click" (shelterClickDecoder w h)
            , SE.on "mousemove" (zoneMoveDecoder Shelter w h)
            , SE.on "mouseover" (D.succeed (DragEnterZone Shelter))
            , SA.style ("max-width:100%;height:auto;border:3px dashed #5a8ab8;border-radius:8px;cursor:"
                ++ (case ( model.paletteSpecies, model.dragging ) of
                        ( _, Just _ ) -> "grabbing"
                        ( Just _, Nothing ) -> "crosshair"
                        _ -> "default"
                   )
                ++ ";background:linear-gradient(180deg,#dfe8f0 0%,#c7d6e5 100%)"
                ++ (case model.dragging of
                        Just d -> if d.currentZone == Shelter then ";box-shadow:0 0 0 3px #4a9b3c" else ""
                        Nothing -> ""
                   )
              )
            ]
            (shelterPatterns
                ++ List.map (viewShelterPlant model) plants
                ++ hoverLayer model plants hoverOverlayShelter
                ++ dragGhost model Shelter
            )
        ]


zoneMoveDecoder : Zone -> Int -> Int -> Decoder Msg
zoneMoveDecoder zone w h =
    D.map2 (\x y -> DragMoveIn zone (clamp 0 (w - 1) x) (clamp 0 (h - 1) y))
        (D.field "offsetX" (D.map round D.float))
        (D.field "offsetY" (D.map round D.float))


-- mousemove sur terrain : convertit offsetX/Y → coords SVG via zoom/pan.
terrainMouseMoveDecoder : Int -> Int -> GardenView -> Decoder Msg
terrainMouseMoveDecoder w h gv =
    D.map2
        (\ox oy ->
            let
                sx = round (gv.panX + toFloat ox / gv.zoom)
                sy = round (gv.panY + toFloat oy / gv.zoom)
            in
            TerrainCursorMove (clamp 0 (w - 1) sx) (clamp 0 (h - 1) sy)
        )
        (D.field "offsetX" (D.map round D.float))
        (D.field "offsetY" (D.map round D.float))


wheelDecoder : Decoder Msg
wheelDecoder =
    D.map3 (\dy ox oy -> GardenZoom dy ox oy)
        (D.field "deltaY" D.float)
        (D.field "offsetX" (D.map round D.float))
        (D.field "offsetY" (D.map round D.float))


-- mousedown sur terrain : si Alt enfoncé → start pan. Sinon NoOp.
panMouseDownDecoder : Decoder Msg
panMouseDownDecoder =
    D.map3 (\alt x y -> GardenPanStart alt x y)
        (D.field "altKey" D.bool)
        (D.field "offsetX" (D.map round D.float))
        (D.field "offsetY" (D.map round D.float))


placementOverlay : Model -> List (Svg.Svg Msg)
placementOverlay model =
    case ( model.paletteSpecies, model.cursorOnTerrain, findSpecies (model.paletteSpecies |> Maybe.withDefault "") model ) of
        ( Just _, Just ( cx, cy ), Just sp ) ->
            let
                spacing = sp.spacingCm
                foeRadius = max 30 (spacing * 2)
                others = plantsFromActions model
                neighborSpacing pl =
                    case findSpecies pl.speciesId model of
                        Just other -> (spacing + other.spacingCm) // 2
                        Nothing -> spacing
                friendSpacing pl = neighborSpacing pl * 7 // 10
                conflictSame =
                    others |> List.any (\pl -> pl.speciesId == sp.id && distanceTo cx cy pl < spacing)
                conflictFoe =
                    others
                        |> List.filter (\pl -> List.member pl.speciesId sp.foes && distanceTo cx cy pl < foeRadius)
                conflictFriend =
                    others
                        |> List.filter
                            (\pl ->
                                List.member pl.speciesId sp.friends
                                    && distanceTo cx cy pl < friendSpacing pl
                            )
                conflictNeutral =
                    others
                        |> List.filter
                            (\pl ->
                                pl.speciesId /= sp.id
                                    && not (List.member pl.speciesId sp.foes)
                                    && not (List.member pl.speciesId sp.friends)
                                    && distanceTo cx cy pl < neighborSpacing pl
                            )
                friendsOk =
                    others
                        |> List.filter
                            (\pl ->
                                List.member pl.speciesId sp.friends
                                    && distanceTo cx cy pl < foeRadius
                                    && distanceTo cx cy pl >= friendSpacing pl
                            )
                hasConflict =
                    conflictSame
                        || not (List.isEmpty conflictFoe)
                        || not (List.isEmpty conflictFriend)
                        || not (List.isEmpty conflictNeutral)
                ringColor =
                    if hasConflict then "#a03030"
                    else if not (List.isEmpty friendsOk) then "#4a9b3c"
                    else "#5a8ab8"
                ringStroke =
                    if hasConflict then "4" else "2"
                label =
                    if conflictSame then "❌ trop près même espèce"
                    else if not (List.isEmpty conflictFoe) then
                        "❌ antagoniste : "
                            ++ String.join ", " (List.map (.speciesId >> speciesShortName) conflictFoe)
                    else if not (List.isEmpty conflictFriend) then
                        "❌ compagnon trop près : "
                            ++ String.join ", " (List.map (.speciesId >> speciesShortName) conflictFriend)
                    else if not (List.isEmpty conflictNeutral) then
                        "❌ trop près de "
                            ++ String.join ", " (List.map (.speciesId >> speciesShortName) conflictNeutral)
                    else if not (List.isEmpty friendsOk) then
                        "✓ compagnons : "
                            ++ String.join ", " (List.map (.speciesId >> speciesShortName) friendsOk)
                    else
                        "espacement " ++ String.fromInt spacing ++ " cm"
            in
            [ Svg.circle
                [ SA.cx (String.fromInt cx)
                , SA.cy (String.fromInt cy)
                , SA.r (String.fromInt (spacing // 2))
                , SA.fill "none"
                , SA.stroke ringColor
                , SA.strokeWidth ringStroke
                , SA.strokeDasharray "5,3"
                , SA.opacity "0.7"
                , SA.style "pointer-events:none"
                ]
                []
            , Svg.text_
                [ SA.x (String.fromInt cx)
                , SA.y (String.fromInt (cy - spacing // 2 - 6))
                , SA.fontSize "11"
                , SA.textAnchor "middle"
                , SA.fill ringColor
                , SA.fontWeight "bold"
                , SA.style "pointer-events:none"
                ]
                [ Svg.text label ]
            ]
                ++ List.map (highlightPlant "#a03030") conflictFoe
                ++ List.map (highlightPlant "#a03030") conflictFriend
                ++ List.map (highlightPlant "#a03030") conflictNeutral
                ++ List.map (highlightPlant "#4a9b3c") friendsOk

        _ -> []


highlightPlant : String -> PlantOnTerrain -> Svg.Svg Msg
highlightPlant color pl =
    Svg.circle
        [ SA.cx (String.fromInt pl.x)
        , SA.cy (String.fromInt pl.y)
        , SA.r "26"
        , SA.fill "none"
        , SA.stroke color
        , SA.strokeWidth "3"
        , SA.opacity "0.6"
        , SA.style "pointer-events:none"
        ]
        []


dragGhost : Model -> Zone -> List (Svg.Svg Msg)
dragGhost model zone =
    case model.dragging of
        Just d ->
            if d.currentZone /= zone then []
            else
                case model.actions |> List.filter (\a -> a.id == d.id) |> List.head of
                    Just a ->
                        let
                            emoji =
                                case a.speciesId of
                                    Just sid -> speciesEmoji sid
                                    Nothing -> "🌱"
                        in
                        [ Svg.text_
                            [ SA.x (String.fromInt d.currentX)
                            , SA.y (String.fromInt (d.currentY + 10))
                            , SA.fontSize "30"
                            , SA.textAnchor "middle"
                            , SA.opacity "0.85"
                            , SA.style "pointer-events:none"
                            ]
                            [ Svg.text emoji ]
                        ]
                    Nothing -> []
        Nothing -> []


shelterPatterns : List (Svg.Svg Msg)
shelterPatterns =
    -- Des petits godets suggérés par des cercles gris.
    let dots = [ 40, 120, 200, 280, 360, 440, 520, 600, 680, 760 ] in
    List.map
        (\cx ->
            Svg.circle
                [ SA.cx (String.fromInt cx)
                , SA.cy "75"
                , SA.r "2"
                , SA.fill "#5a3a22"
                , SA.opacity "0.15"
                ]
                []
        )
        dots


shelterClickDecoder : Int -> Int -> Decoder Msg
shelterClickDecoder w h =
    D.map2 (\x y -> PlaceInShelter (clamp 0 (w - 1) x) (clamp 0 (h - 1) y))
        (D.field "offsetX" (D.map round D.float))
        (D.field "offsetY" (D.map round D.float))


shelterPlantsFromActions : Model -> List PlantOnTerrain
shelterPlantsFromActions model =
    let
        today = effectiveToday model
        daysSince d = daysBetween d today
    in
    model.actions
        |> List.filterMap
            (\a ->
                case ( a.speciesId, a.gridX, a.gridY ) of
                    ( Just sid, Just x, Just y ) ->
                        if a.kind == "semis_abri" then
                            let
                                days = daysSince a.date
                                cycle = 40 -- durée indicative pépinière
                                progress = clamp 0 1 (toFloat days / toFloat cycle)
                                state =
                                    if progress < 0.2 then TileSown sid
                                    else if progress < 1.0 then TileGrowing sid progress
                                    else TileMature sid
                            in
                            Just
                                { id = a.id, speciesId = sid, x = x, y = y
                                , date = a.date, progress = progress, state = state
                                }
                        else Nothing
                    _ -> Nothing
            )


viewShelterPlant : Model -> PlantOnTerrain -> Svg.Svg Msg
viewShelterPlant model p =
    let
        emoji = stateEmoji p.state
        size = plantSize p.state
        isDragged = (model.dragging |> Maybe.map .id) == Just p.id
        isMoving = model.movingPlant == Just p.id || isDragged
        ready = p.progress >= 0.6
        opacity = if isMoving then "0.35" else "1"
        name = speciesShortName p.speciesId
    in
    Svg.g
        [ SE.onMouseOver (HoverPlant (Just p.id))
        , SE.onMouseOut (HoverPlant Nothing)
        , SE.on "mousedown" (plantDragDecoder p.id Shelter)
        , SE.stopPropagationOn "click" (D.succeed ( NoOp, True ))
        , SA.style ("cursor:grab" ++ (if model.dragging /= Nothing && not isDragged then ";pointer-events:none" else ""))
        , SA.opacity opacity
        ]
        ([ Svg.circle
            [ SA.cx (String.fromInt p.x)
            , SA.cy (String.fromInt p.y)
            , SA.r (String.fromInt (size // 2 + 3))
            , SA.fill (if ready then "#c5e1a5" else "#e8d8c0")
            , SA.opacity "0.9"
            , SA.stroke (if ready then "#4a9b3c" else "#8b6e3d")
            , SA.strokeWidth (if ready then "2" else "1.5")
            ]
            []
         , plantGlyph True p.x p.y size (if emoji == "" then speciesEmoji p.speciesId else emoji)
         ]
        )


-- Taille d'affichage d'un plant selon son état.
plantSize : TileState -> Int
plantSize st =
    case st of
        TileSown _ -> 22
        TileGrowing _ pr -> 22 + round (pr * 10)
        TileMature _ -> 32
        _ -> 22


-- Overlay hover dessiné en couche haute (après tous les plants) pour ne
-- jamais être masqué par un plant voisin dessiné plus tard dans le SVG.
hoverLayer : Model -> List PlantOnTerrain -> (PlantOnTerrain -> Int -> List (Svg.Svg Msg)) -> List (Svg.Svg Msg)
hoverLayer model plants overlay =
    case ( model.hoverPlant, model.dragging ) of
        ( Just hid, Nothing ) ->
            plants
                |> List.filter (\p -> p.id == hid)
                |> List.concatMap (\p -> overlay p (plantSize p.state))

        _ -> []


-- Le glyph emoji 🌱 (U+1F331) se dessine avec un pot en terre cuite orange
-- dans toutes les fonts emoji système (Noto Color, Apple, Twemoji…). Comme
-- on ne peut pas "extraire" la pousse sans le pot, on remplace 🌱/🌿 par
-- un sprout SVG natif (tige + feuilles, full vert, sans pot). Les autres
-- emojis (espèces : 🍅 🥕 …) gardent leur rendu emoji standard.
plantGlyph : Bool -> Int -> Int -> Int -> String -> Svg.Svg Msg
plantGlyph _ cx cy size emoji =
    if emoji == "🌱" || emoji == "🌿" then
        sproutSvg cx cy size
    else
        Svg.text_
            [ SA.x (String.fromInt cx)
            , SA.y (String.fromInt (cy + size // 3))
            , SA.fontSize (String.fromInt size)
            , SA.textAnchor "middle"
            , SA.style "pointer-events:none"
            ]
            [ Svg.text emoji ]


sproutSvg : Int -> Int -> Int -> Svg.Svg Msg
sproutSvg cx cy size =
    let
        s = toFloat size
        cxf = toFloat cx
        cyf = toFloat cy
        -- tige verticale
        stemX = cxf
        stemTop = cyf - s * 0.05
        stemBot = cyf + s * 0.35
        -- feuille gauche (ellipse penchée)
        leafLX = cxf - s * 0.22
        leafLY = cyf - s * 0.05
        -- feuille droite
        leafRX = cxf + s * 0.22
        leafRY = cyf - s * 0.05
        f x = String.fromFloat x
    in
    Svg.g [ SA.style "pointer-events:none" ]
        [ Svg.line
            [ SA.x1 (f stemX), SA.y1 (f stemBot)
            , SA.x2 (f stemX), SA.y2 (f stemTop)
            , SA.stroke "#3a7a2b", SA.strokeWidth (f (s * 0.07))
            , SA.strokeLinecap "round"
            ]
            []
        , Svg.ellipse
            [ SA.cx (f leafLX), SA.cy (f leafLY)
            , SA.rx (f (s * 0.22)), SA.ry (f (s * 0.13))
            , SA.fill "#5fae3a"
            , SA.transform ("rotate(-35 " ++ f leafLX ++ " " ++ f leafLY ++ ")")
            ]
            []
        , Svg.ellipse
            [ SA.cx (f leafRX), SA.cy (f leafRY)
            , SA.rx (f (s * 0.22)), SA.ry (f (s * 0.13))
            , SA.fill "#5fae3a"
            , SA.transform ("rotate(35 " ++ f leafRX ++ " " ++ f leafRY ++ ")")
            ]
            []
        ]


hoverOverlayShelter : PlantOnTerrain -> Int -> List (Svg.Svg Msg)
hoverOverlayShelter p size =
    [ Svg.text_
        [ SA.x (String.fromInt p.x)
        , SA.y (String.fromInt (p.y - size // 2 - 22))
        , SA.fontSize "10"
        , SA.textAnchor "middle"
        , SA.fill "#2a1810"
        , SA.fontWeight "bold"
        ]
        [ Svg.text (speciesShortName p.speciesId ++ " · " ++ String.fromInt (round (p.progress * 100)) ++ "%") ]
    , Svg.text_
        [ SA.x (String.fromInt p.x)
        , SA.y (String.fromInt (p.y - size // 2 - 6))
        , SA.fontSize "13"
        , SA.fill "#2a1810"
        , SE.stopPropagationOn "click" (D.succeed ( StartMoving p.id, True ))
        , SA.style "cursor:pointer"
        ]
        [ Svg.text "✎" ]
    , Svg.text_
        [ SA.x (String.fromInt (p.x + 20))
        , SA.y (String.fromInt (p.y - size // 2 - 6))
        , SA.fontSize "13"
        , SA.fill "#a03030"
        , SE.stopPropagationOn "click" (D.succeed ( DeletePlant p.id, True ))
        , SA.style "cursor:pointer"
        ]
        [ Svg.text "✕" ]
    ]


viewTerrain : Model -> Html Msg
viewTerrain model =
    let
        w = 800
        h = 560
        widthM = toFloat w / 100  -- 1 px = 1 cm
        heightM = toFloat h / 100
        areaM2 = round1 (widthM * heightM)
        plantCount = List.length (plantsFromActions model)
        hint =
            case model.paletteSpecies of
                Just sid ->
                    "🖱 Clique sur le terrain pour poser " ++ speciesShortName sid ++ " · Échap pour annuler"
                Nothing ->
                    "Clique sur une espèce de la palette pour la poser. Glisse un plant de l'abri vers le jardin pour repiquer."
        plants = plantsFromActions model
    in
    let
        gv = model.gardenView
        vbW = toFloat w / gv.zoom
        vbH = toFloat h / gv.zoom
        vbStr =
            String.fromFloat gv.panX ++ " "
                ++ String.fromFloat gv.panY ++ " "
                ++ String.fromFloat vbW ++ " "
                ++ String.fromFloat vbH
        cursorStyle =
            case ( model.panning, model.paletteSpecies, model.dragging ) of
                ( Just _, _, _ ) -> "grabbing"
                ( _, _, Just _ ) -> "grabbing"
                ( _, Just _, _ ) -> "crosshair"
                _ -> "default"
    in
    div [ A.class "panel" ]
        [ h2 []
            [ text ("🌾 Mon jardin · " ++ String.fromFloat widthM ++ " m × " ++ String.fromFloat heightM ++ " m = " ++ String.fromFloat areaM2 ++ " m²")
            ]
        , p [ A.style "font-size" "0.78rem", A.style "color" "#5a3a22" ]
            [ text (hint ++ " · " ++ String.fromInt plantCount ++ " plant(s) installé(s) · molette = zoom · Alt+drag = pan") ]
        , div [ A.style "display" "flex", A.style "gap" "0.4rem", A.style "margin-bottom" "0.3rem", A.style "font-size" "0.76rem", A.style "align-items" "center" ]
            [ span [ A.style "color" "#5a3a22" ]
                [ text ("zoom " ++ String.fromInt (round (gv.zoom * 100)) ++ "%") ]
            , button
                [ E.onClick GardenZoomReset
                , A.style "padding" "2px 8px", A.style "font-size" "0.72rem"
                , A.style "background" "#fff6de", A.style "color" "#5a3a22"
                , A.style "border" "1px solid #d4b85a", A.style "border-radius" "3px"
                , A.style "cursor" "pointer"
                ]
                [ text "⟲ reset" ]
            ]
        , Svg.svg
            [ SA.viewBox vbStr
            , SA.width (String.fromInt w)
            , SE.on "click" (terrainClickDecoder w h gv)
            , SE.on "mousemove" (terrainMouseMoveDecoder w h gv)
            , SE.on "mouseover" (D.succeed (DragEnterZone Terrain))
            , SE.onMouseOut TerrainCursorLeave
            , SE.preventDefaultOn "wheel" (wheelDecoder |> D.map (\m -> ( m, True )))
            , SE.preventDefaultOn "mousedown" (panMouseDownDecoder |> D.map (\m -> ( m, True )))
            , SA.style ("max-width:100%;height:auto;border:3px solid #6a4a2a;border-radius:8px;cursor:"
                ++ cursorStyle
                ++ ";background:" ++ terrainBackground model
                ++ (case model.dragging of
                        Just d -> if d.currentZone == Terrain then ";box-shadow:0 0 0 3px #4a9b3c" else ""
                        Nothing -> ""
                   )
              )
            ]
            (terrainPatterns
                ++ List.map (viewPlantOnTerrain model) plants
                ++ hoverLayer model plants hoverOverlayTerrain
                ++ dragGhost model Terrain
                ++ placementOverlay model
            )
        ]


terrainBackground : Model -> String
terrainBackground _ =
    -- Teinte terre riche avec léger dégradé.
    "radial-gradient(ellipse at center, #b89063 0%, #8b6e3d 100%)"


terrainPatterns : List (Svg.Svg Msg)
terrainPatterns =
    -- Petite texture : quelques points plus foncés pour figurer le relief.
    let
        dots =
            [ ( 80, 120 ), ( 200, 60 ), ( 350, 180 ), ( 500, 90 ), ( 650, 220 )
            , ( 120, 340 ), ( 290, 420 ), ( 480, 350 ), ( 620, 470 ), ( 750, 180 )
            , ( 40, 480 ), ( 230, 500 ), ( 390, 520 ), ( 560, 540 ), ( 700, 510 )
            ]
    in
    List.map
        (\( cx, cy ) ->
            Svg.circle
                [ SA.cx (String.fromInt cx)
                , SA.cy (String.fromInt cy)
                , SA.r "4"
                , SA.fill "#6a4a2a"
                , SA.opacity "0.25"
                ]
                []
        )
        dots


-- Décodeur qui extrait (offsetX, offsetY) d'un clic et les clampe aux dimensions du terrain.


terrainClickDecoder : Int -> Int -> GardenView -> Decoder Msg
terrainClickDecoder w h gv =
    D.map2
        (\ox oy ->
            let
                sx = round (gv.panX + toFloat ox / gv.zoom)
                sy = round (gv.panY + toFloat oy / gv.zoom)
            in
            PlaceAtPixel (clamp 0 (w - 1) sx) (clamp 0 (h - 1) sy)
        )
        (D.field "offsetX" (D.map round D.float))
        (D.field "offsetY" (D.map round D.float))


-- Un "plant" est une action semis/repiquage posée sur le terrain.


type alias PlantOnTerrain =
    { id : Int
    , speciesId : String
    , x : Int
    , y : Int
    , date : String
    , progress : Float
    , state : TileState
    }


plantsFromActions : Model -> List PlantOnTerrain
plantsFromActions model =
    let
        today = effectiveToday model
        daysSince d = daysBetween d today
    in
    model.actions
        |> List.filterMap
            (\a ->
                case ( a.speciesId, a.gridX, a.gridY ) of
                    ( Just sid, Just x, Just y ) ->
                        if List.member a.kind [ "semis_direct", "repiquage" ] then
                            let
                                days = daysSince a.date
                                cycle =
                                    case model.calendar of
                                        Just cal ->
                                            cal.species
                                                |> List.filter (\sl -> sl.species.id == sid)
                                                |> List.head
                                                |> Maybe.map (\sl -> sl.species.daysToHarvest)
                                                |> Maybe.withDefault 90
                                        Nothing -> 90
                                progress = clamp 0 1 (toFloat days / toFloat (max 1 cycle))
                                state =
                                    if progress < 0.05 then TileSown sid
                                    else if progress < 0.95 then TileGrowing sid progress
                                    else TileMature sid
                            in
                            Just
                                { id = a.id
                                , speciesId = sid
                                , x = x
                                , y = y
                                , date = a.date
                                , progress = progress
                                , state = state
                                }
                        else
                            Nothing
                    _ -> Nothing
            )


viewPlantOnTerrain : Model -> PlantOnTerrain -> Svg.Svg Msg
viewPlantOnTerrain model p =
    let
        emoji = stateEmoji p.state
        size = plantSize p.state
        isHover = model.hoverPlant == Just p.id && model.dragging == Nothing
        isDragged = (model.dragging |> Maybe.map .id) == Just p.id
        isMoving = model.movingPlant == Just p.id || isDragged
        opacity = if isMoving then "0.35" else "1"
        bgColor =
            case p.state of
                TileSown _ -> "#d4e2b8"
                TileGrowing _ _ -> "#aed581"
                TileMature _ -> "#f3c04a"
                _ -> "#e8d39a"
        paillage = hasRecentPaillage model p
        spacingRadius =
            case findSpecies p.speciesId model of
                Just sp -> sp.spacingCm // 2
                Nothing -> 20
        hasSpacingConflict =
            plantsFromActions model
                |> List.any
                    (\other ->
                        other.id /= p.id
                            && distanceTo p.x p.y other < spacingRadius * 2
                    )
        spacingColor =
            if hasSpacingConflict then "#a03030" else "#4a9b3c"
    in
    Svg.g
        [ SE.onMouseOver (HoverPlant (Just p.id))
        , SE.onMouseOut (HoverPlant Nothing)
        , SE.on "mousedown" (plantDragDecoder p.id Terrain)
        , SE.stopPropagationOn "click" (D.succeed ( NoOp, True ))
        , SA.style "cursor:grab"
        , SA.opacity opacity
        ]
        ([ -- Frontière plantation : rouge si voisin trop proche, vert sinon
           Svg.circle
            [ SA.cx (String.fromInt p.x)
            , SA.cy (String.fromInt p.y)
            , SA.r (String.fromInt spacingRadius)
            , SA.fill spacingColor
            , SA.fillOpacity "0.10"
            , SA.stroke spacingColor
            , SA.strokeWidth "2"
            , SA.strokeDasharray "5,3"
            , SA.opacity "0.85"
            , SA.style "pointer-events:none"
            ]
            []
         , -- Petit anneau paillage : très proche du plant, n'envahit pas
           Svg.circle
            [ SA.cx (String.fromInt p.x)
            , SA.cy (String.fromInt p.y)
            , SA.r (String.fromInt (size // 2 + 4))
            , SA.fill "none"
            , SA.stroke (if paillage then "#8b6e3d" else "none")
            , SA.strokeWidth "1.5"
            , SA.strokeDasharray "3,2"
            , SA.opacity (if paillage then "0.7" else "0")
            ]
            []
         , Svg.circle
            [ SA.cx (String.fromInt p.x)
            , SA.cy (String.fromInt p.y)
            , SA.r (String.fromInt (size // 2 + 1))
            , SA.fill bgColor
            , SA.stroke (if isHover then "#c06020" else "#3d2818")
            , SA.strokeWidth (if isHover then "2" else "1")
            , SA.opacity "0.95"
            ]
            []
         , plantGlyph True p.x p.y size (if emoji == "" then speciesEmoji p.speciesId else emoji)
         ]
        )


hasRecentPaillage : Model -> PlantOnTerrain -> Bool
hasRecentPaillage model p =
    let today = effectiveToday model in
    model.actions
        |> List.any
            (\a ->
                a.kind == "paillage"
                    && a.gridX == Just p.x
                    && a.gridY == Just p.y
                    && daysBetween a.date today < 45
            )


plantDragDecoder : Int -> Zone -> Decoder Msg
plantDragDecoder id zone =
    D.map2 (\x y -> DragStart id zone x y)
        (D.field "offsetX" (D.map round D.float))
        (D.field "offsetY" (D.map round D.float))


hoverOverlayTerrain : PlantOnTerrain -> Int -> List (Svg.Svg Msg)
hoverOverlayTerrain p size =
    let
        labelText = speciesShortName p.speciesId ++ " · " ++ String.fromInt (round (p.progress * 100)) ++ "%"
        labelW = String.length labelText * 7 + 10
    in
    [ Svg.rect
        [ SA.x (String.fromInt (p.x - labelW // 2))
        , SA.y (String.fromInt (p.y - size // 2 - 28))
        , SA.width (String.fromInt labelW)
        , SA.height "16"
        , SA.fill "#fff6de", SA.stroke "#5a3a22", SA.rx "3"
        , SA.opacity "0.97"
        , SA.style "pointer-events:none"
        ]
        []
    , Svg.text_
        [ SA.x (String.fromInt p.x)
        , SA.y (String.fromInt (p.y - size // 2 - 16))
        , SA.fontSize "11"
        , SA.textAnchor "middle"
        , SA.fill "#2a1810"
        , SA.fontWeight "bold"
        , SA.style "pointer-events:none"
        ]
        [ Svg.text labelText ]
    , Svg.text_
        [ SA.x (String.fromInt (p.x - 18))
        , SA.y (String.fromInt (p.y - size // 2 - 2))
        , SA.fontSize "14"
        , SA.fill "#2a1810"
        , SE.stopPropagationOn "click" (D.succeed ( StartMoving p.id, True ))
        , SA.style "cursor:pointer"
        ]
        [ Svg.text "✎" ]
    , Svg.text_
        [ SA.x (String.fromInt (p.x + 18))
        , SA.y (String.fromInt (p.y - size // 2 - 2))
        , SA.fontSize "14"
        , SA.fill "#a03030"
        , SE.stopPropagationOn "click" (D.succeed ( DeletePlant p.id, True ))
        , SA.style "cursor:pointer"
        ]
        [ Svg.text "✕" ]
    ]


viewPalette : Model -> Html Msg
viewPalette model =
    let
        speciesOpt =
            case model.calendar of
                Just cal -> cal.species
                Nothing -> []
        doy = isoToDoy (effectiveToday model)

        indoorOpen sl =
            case sl.indoorSowLocal of
                Just wnd -> doyInWindow doy wnd
                Nothing -> False

        directOpen sl =
            case sl.directSowLocal of
                Just wnd -> doyInWindow doy wnd
                Nothing -> False

        recoIndoor = List.filter indoorOpen speciesOpt
        recoDirect = List.filter directOpen speciesOpt
        recoBoth = List.filter (\sl -> indoorOpen sl || directOpen sl) speciesOpt |> List.map (.species >> .id)
        other = List.filter (\sl -> not (List.member sl.species.id recoBoth)) speciesOpt
    in
    div [ A.class "panel" ]
        [ h2 [] [ text "🌱 Palette" ]
        , p [ A.style "font-size" "0.78rem", A.style "color" "#5a3a22" ]
            [ text "Sélectionne une espèce, clique l'abri 🌡 ou le terrain 🌱." ]
        , case model.movingPlant of
            Just _ ->
                div [ A.style "padding" "0.5rem", A.style "background" "#fef6c8", A.style "border" "2px solid #d4a033", A.style "border-radius" "4px", A.style "margin-bottom" "0.5rem" ]
                    [ text "✎ Déplacement en cours — clique la nouvelle position (abri ou terrain) "
                    , button [ E.onClick CancelMoving, A.style "margin-left" "0.3rem" ] [ text "Annuler" ]
                    ]
            Nothing -> text ""
        , if not (List.isEmpty recoIndoor) then
            div [ A.style "margin-bottom" "0.5rem" ]
                [ div [ A.style "font-size" "0.75rem", A.style "color" "#c66339", A.style "margin-bottom" "0.2rem", A.style "font-weight" "bold" ]
                    [ text ("🌡 Sous abri — fenêtre ouverte (" ++ String.fromInt (List.length recoIndoor) ++ ")") ]
                , div [ A.style "display" "flex", A.style "flex-wrap" "wrap", A.style "gap" "0.3rem" ]
                    (List.map (paletteItem model) recoIndoor)
                ]
          else text ""
        , if not (List.isEmpty recoDirect) then
            div [ A.style "margin-bottom" "0.5rem" ]
                [ div [ A.style "font-size" "0.75rem", A.style "color" "#5a8a35", A.style "margin-bottom" "0.2rem", A.style "font-weight" "bold" ]
                    [ text ("🌱 En pleine terre — fenêtre ouverte (" ++ String.fromInt (List.length recoDirect) ++ ")") ]
                , div [ A.style "display" "flex", A.style "flex-wrap" "wrap", A.style "gap" "0.3rem" ]
                    (List.map (paletteItem model) recoDirect)
                ]
          else text ""
        , div [ A.style "font-size" "0.75rem", A.style "color" "#8b6e3d", A.style "margin" "0.3rem 0 0.2rem 0" ]
            [ text ("Autres espèces (" ++ String.fromInt (List.length other) ++ ")") ]
        , div
            [ A.style "display" "flex", A.style "flex-wrap" "wrap", A.style "gap" "0.3rem"
            , A.style "max-height" "240px", A.style "overflow-y" "auto"
            ]
            (List.map (paletteItem model) other)
        , case model.paletteSpecies of
            Just sid ->
                div [ A.style "margin-top" "0.6rem", A.style "padding" "0.4rem", A.style "background" "#fff6de", A.style "border-radius" "4px", A.style "font-size" "0.82rem" ]
                    [ text ("Sélection : " ++ speciesEmoji sid ++ " " ++ speciesShortName sid ++ " ")
                    , button [ E.onClick ClearPalette, A.style "margin-left" "0.4rem", A.style "font-size" "0.72rem" ] [ text "✕" ]
                    ]
            Nothing -> text ""
        ]


paletteItem : Model -> SpeciesLocal -> Html Msg
paletteItem model sl =
    let
        selected = model.paletteSpecies == Just sl.species.id
    in
    div
        [ E.onClick (SelectPaletteSpecies sl.species.id)
        , A.title (sl.species.nameFr ++ " (" ++ sl.species.nameLatin ++ ")")
        , A.style "width" "60px", A.style "height" "56px"
        , A.style "display" "flex", A.style "flex-direction" "column"
        , A.style "align-items" "center", A.style "justify-content" "center"
        , A.style "cursor" "pointer"
        , A.style "border-radius" "6px"
        , A.style "background" (if selected then "#d4a033" else "#fff6de")
        , A.style "border" (if selected then "2px solid #a03030" else "1px solid #d4b85a")
        , A.style "transition" "all 0.15s"
        , A.style "padding" "2px"
        ]
        [ span [ A.style "font-size" "22px", A.style "line-height" "1" ] [ text (speciesEmoji sl.species.id) ]
        , span [ A.style "font-size" "9px", A.style "color" "#3d2818", A.style "margin-top" "2px", A.style "text-align" "center" ]
            [ text (speciesShortName sl.species.id) ]
        ]


viewTile : Model -> Int -> Int -> Int -> Int -> Svg.Svg Msg
viewTile model tile gap c r =
    let
        x = c * (tile + gap)
        y = r * (tile + gap)
        state = deriveStateAt model c r
        bgColor = stateBackground state
        emoji = stateEmoji state
        isActiveForm = model.actionForm.gridX == String.fromInt c && model.actionForm.gridY == String.fromInt r
        emojiSize = tile // 2
    in
    Svg.g [ SE.onClick (SelectTile c r), SA.style "cursor:pointer" ]
        ([ Svg.rect
            [ SA.x (String.fromInt x)
            , SA.y (String.fromInt y)
            , SA.width (String.fromInt tile)
            , SA.height (String.fromInt tile)
            , SA.fill bgColor
            , SA.stroke (if isActiveForm then "#c06020" else "#6a4a2a")
            , SA.strokeWidth (if isActiveForm then "2.5" else "0.5")
            , SA.opacity "0.85"
            ]
            []
         ]
            ++ (if emoji == "" then
                    []
                else
                    [ Svg.text_
                        [ SA.x (String.fromInt (x + tile // 2))
                        , SA.y (String.fromInt (y + tile // 2 + emojiSize // 3))
                        , SA.fontSize (String.fromInt emojiSize)
                        , SA.textAnchor "middle"
                        ]
                        [ Svg.text emoji ]
                    ]
               )
        )


deriveStateAt : Model -> Int -> Int -> TileState
deriveStateAt model c r =
    let
        today = effectiveToday model
        tileActions =
            model.actions
                |> List.filter
                    (\a ->
                        a.gridX == Just c && a.gridY == Just r
                    )

        latest = List.head tileActions

        lastSeed =
            tileActions
                |> List.filter (\a -> List.member a.kind [ "semis_direct", "semis_abri", "repiquage" ])
                |> List.head

        daysSince d = daysBetween d today
    in
    case latest of
        Nothing -> TileEmpty

        Just last ->
            if last.kind == "arrachage" then TileEmpty
            else if last.kind == "recolte" && daysSince last.date < 14 then
                TileHarvested (last.speciesId |> Maybe.withDefault "")
            else
                case lastSeed of
                    Nothing ->
                        if List.member last.kind [ "compost", "paillage" ] && daysSince last.date < 30 then
                            TileTilled
                        else
                            TileEmpty

                    Just seed ->
                        let
                            sp = seed.speciesId |> Maybe.withDefault ""
                            days = daysSince seed.date
                            cycleDays =
                                case model.calendar of
                                    Just cal ->
                                        cal.species
                                            |> List.filter (\sl -> sl.species.id == sp)
                                            |> List.head
                                            |> Maybe.map (\sl -> sl.species.daysToHarvest)
                                            |> Maybe.withDefault 90
                                    Nothing -> 90
                            progress = toFloat days / toFloat (max 1 cycleDays)
                        in
                        if progress < 0.05 then TileSown sp
                        else if progress < 0.95 then TileGrowing sp (clamp 0 1 progress)
                        else TileMature sp


viewParcelTile : Model -> Int -> Int -> Parcel -> Svg.Svg Msg
viewParcelTile model tile gap p =
    let
        x = p.gridX * (tile + gap)
        y = p.gridY * (tile + gap)
        w = p.gridW * (tile + gap) - gap
        h = p.gridH * (tile + gap) - gap
        isSelected = model.editingParcel == Just p.id
        state = deriveState model p
        bgColor = stateBackground state
        emoji = stateEmoji state
        cx = x + w // 2
        cy = y + h // 2
        emojiSize = min w h // 2 |> max 14 |> min 32
    in
    Svg.g [ SE.onClick (EditParcel p), SA.style "cursor:pointer" ]
        ([ Svg.rect
            [ SA.x (String.fromInt x)
            , SA.y (String.fromInt y)
            , SA.width (String.fromInt w)
            , SA.height (String.fromInt h)
            , SA.fill bgColor
            , SA.stroke (if isSelected then "#c06020" else "#3d2818")
            , SA.strokeWidth (if isSelected then "3" else "1.5")
            , SA.rx "4"
            , SA.opacity "0.92"
            ]
            []
         , Svg.text_
            [ SA.x (String.fromInt (x + 6))
            , SA.y (String.fromInt (y + 14))
            , SA.fontSize "11"
            , SA.fontWeight "bold"
            , SA.fill "#2a1810"
            ]
            [ Svg.text p.name ]
         , Svg.text_
            [ SA.x (String.fromInt (x + 6))
            , SA.y (String.fromInt (y + h - 6))
            , SA.fontSize "9"
            , SA.fill "#3d2818"
            , SA.opacity "0.85"
            ]
            [ Svg.text (stateLabel state) ]
         ]
            ++ (if emoji == "" then
                    []
                else
                    [ Svg.text_
                        [ SA.x (String.fromInt cx)
                        , SA.y (String.fromInt (cy + emojiSize // 3))
                        , SA.fontSize (String.fromInt emojiSize)
                        , SA.textAnchor "middle"
                        ]
                        [ Svg.text emoji ]
                    ]
               )
        )


lastActionSummary : Model -> Parcel -> String
lastActionSummary model p =
    let
        last =
            model.actions
                |> List.filter (\a -> a.parcelId == Just p.id)
                |> List.head
    in
    case last of
        Nothing -> "vide"
        Just a ->
            let
                sp = a.speciesId |> Maybe.withDefault ""
                kind = actionKindLabel a.kind
            in
            if sp == "" then
                kind
            else
                kind ++ " " ++ sp


-- STATE DERIVATION


deriveState : Model -> Parcel -> TileState
deriveState model p =
    let
        parcelActions =
            model.actions
                |> List.filter (\a -> a.parcelId == Just p.id)

        latest = List.head parcelActions

        -- Dernière action de semis/repiquage sur la parcelle
        lastSeed =
            parcelActions
                |> List.filter (\a -> List.member a.kind [ "semis_direct", "semis_abri", "repiquage" ])
                |> List.head

        -- Jours écoulés depuis aujourd'hui
        daysSince date = daysBetween date model.today
    in
    case latest of
        Nothing -> TileEmpty

        Just last ->
            if last.kind == "arrachage" then
                TileEmpty

            else if last.kind == "recolte" && daysSince last.date < 14 then
                TileHarvested (last.speciesId |> Maybe.withDefault "")

            else
                case lastSeed of
                    Nothing ->
                        if List.member last.kind [ "compost", "paillage" ] && daysSince last.date < 30 then
                            TileTilled
                        else
                            TileEmpty

                    Just seed ->
                        let
                            sp = seed.speciesId |> Maybe.withDefault ""
                            days = daysSince seed.date
                            cycleDays =
                                case model.calendar of
                                    Just cal ->
                                        cal.species
                                            |> List.filter (\sl -> sl.species.id == sp)
                                            |> List.head
                                            |> Maybe.map (\sl -> sl.species.daysToHarvest)
                                            |> Maybe.withDefault 90

                                    Nothing -> 90

                            progress = toFloat days / toFloat (max 1 cycleDays)
                        in
                        if progress < 0.05 then TileSown sp
                        else if progress < 0.95 then TileGrowing sp (clamp 0 1 progress)
                        else TileMature sp


daysBetween : String -> String -> Int
daysBetween from to =
    isoToOrdinal to - isoToOrdinal from


type Season
    = Spring
    | Summer
    | Autumn
    | Winter


seasonOf : String -> Season
seasonOf iso =
    case String.split "-" iso of
        [ _, m, _ ] ->
            case String.toInt m of
                Just month ->
                    if month >= 3 && month <= 5 then Spring
                    else if month >= 6 && month <= 8 then Summer
                    else if month >= 9 && month <= 11 then Autumn
                    else Winter
                Nothing -> Spring
        _ -> Spring


seasonIcon : Season -> String
seasonIcon s =
    case s of
        Spring -> "🌸"
        Summer -> "☀"
        Autumn -> "🍂"
        Winter -> "❄"


seasonLabel : Season -> String
seasonLabel s =
    case s of
        Spring -> "Printemps"
        Summer -> "Été"
        Autumn -> "Automne"
        Winter -> "Hiver"


seasonBg : Season -> String
seasonBg s =
    case s of
        Spring -> "linear-gradient(180deg,#f5ecd6 0%,#e3edd1 100%)"
        Summer -> "linear-gradient(180deg,#fef7d6 0%,#f6e1a3 100%)"
        Autumn -> "linear-gradient(180deg,#f5e0c4 0%,#e6bc9a 100%)"
        Winter -> "linear-gradient(180deg,#e8eef5 0%,#c9d7e5 100%)"


seasonMonths : Season -> List Int
seasonMonths s =
    case s of
        Spring -> [ 3, 4, 5 ]
        Summer -> [ 6, 7, 8 ]
        Autumn -> [ 9, 10, 11 ]
        Winter -> [ 12, 1, 2 ]


speciesActiveInSeason : Season -> SpeciesLocal -> Bool
speciesActiveInSeason season sl =
    let
        months = seasonMonths season
        windowActive w =
            List.any
                (\m ->
                    let
                        doy = monthFirstDoy m
                    in
                    doyInWindow doy w
                )
                months
        anyActive =
            (case sl.indoorSowLocal of
                Just w -> windowActive w
                Nothing -> False
            )
                || (case sl.directSowLocal of
                        Just w -> windowActive w
                        Nothing -> False
                   )
                || (case sl.transplantLocal of
                        Just w -> windowActive w
                        Nothing -> False
                   )
                || windowActive sl.harvestLocal
    in
    anyActive


monthFirstDoy : Int -> Int
monthFirstDoy m =
    case m of
        1 -> 1
        2 -> 32
        3 -> 60
        4 -> 91
        5 -> 121
        6 -> 152
        7 -> 182
        8 -> 213
        9 -> 244
        10 -> 274
        11 -> 305
        12 -> 335
        _ -> 1


doyInWindow : Int -> CalendarWindow -> Bool
doyInWindow doy w =
    if w.doyStart <= w.doyEnd then
        doy >= w.doyStart && doy <= w.doyEnd
    else
        doy >= w.doyStart || doy <= w.doyEnd


-- Date effective pour les calculs : DOY visualisé sur l'année de today.
effectiveToday : Model -> String
effectiveToday m =
    let
        todayDoy = isoToDoy m.today
    in
    if m.viewDoy == todayDoy then
        m.today
    else
        addDaysToIso m.today (m.viewDoy - todayDoy)


-- Décalage virtuel par rapport à today, en jours (peut être négatif).
viewOffset : Model -> Int
viewOffset m = m.viewDoy - isoToDoy m.today


addDaysToIso : String -> Int -> String
addDaysToIso iso days =
    let
        ord = isoToOrdinal iso + days
        year = ord // 365
        doy = ord - year * 365 + 1
        ( month, dayInMonth ) = doyToMd doy
        pad n = String.padLeft 2 '0' (String.fromInt n)
    in
    String.fromInt year ++ "-" ++ pad month ++ "-" ++ pad dayInMonth


-- Décompose un jour de l'année (1..365) en (mois 1..12, jour 1..31).
doyToMd : Int -> ( Int, Int )
doyToMd doy =
    let
        cumul = [ 0, 31, 59, 90, 120, 151, 181, 212, 243, 273, 304, 334, 365 ]
        go idx rest =
            case rest of
                a :: b :: tail ->
                    if doy > a && doy <= b then
                        ( idx, doy - a )
                    else
                        go (idx + 1) (b :: tail)
                _ -> ( 12, doy - 334 )
    in
    go 1 cumul


isoToOrdinal : String -> Int
isoToOrdinal s =
    case String.split "-" s of
        [ y, m, d ] ->
            let
                year = String.toInt y |> Maybe.withDefault 2000
                month = String.toInt m |> Maybe.withDefault 1
                day = String.toInt d |> Maybe.withDefault 1
                -- Cumul mois (non-bissextile, suffisant pour diff courte).
                cumul = [ 0, 31, 59, 90, 120, 151, 181, 212, 243, 273, 304, 334 ]
                monthOffset =
                    cumul
                        |> List.drop (max 0 (month - 1))
                        |> List.head
                        |> Maybe.withDefault 0
            in
            year * 365 + monthOffset + (day - 1)

        _ -> 0


speciesEmoji : String -> String
speciesEmoji id =
    case id of
        "tomato" -> "🍅"
        "tomato_cherry" -> "🍅"
        "pepper" -> "🫑"
        "eggplant" -> "🍆"
        "zucchini" -> "🥒"
        "butternut_squash" -> "🎃"
        "pumpkin" -> "🎃"
        "cucumber" -> "🥒"
        "melon" -> "🍈"
        "carrot" -> "🥕"
        "radish" -> "🌶"
        "turnip" -> "🧅"
        "beet" -> "🍠"
        "parsnip" -> "🥕"
        "celeriac" -> "🥔"
        "potato" -> "🥔"
        "jerusalem_artichoke" -> "🥔"
        "lettuce" -> "🥬"
        "lambs_lettuce" -> "🌿"
        "spinach" -> "🌿"
        "arugula" -> "🌿"
        "chard" -> "🥬"
        "kale" -> "🥬"
        "cabbage" -> "🥬"
        "cauliflower" -> "🥦"
        "broccoli" -> "🥦"
        "brussels_sprout" -> "🥬"
        "bean_green" -> "🫛"
        "bean_dry" -> "🫘"
        "pea" -> "🫛"
        "broad_bean" -> "🫘"
        "onion" -> "🧅"
        "shallot" -> "🧅"
        "garlic" -> "🧄"
        "leek" -> "🌱"
        "chives" -> "🌿"
        "basil" -> "🌿"
        "parsley" -> "🌿"
        "coriander" -> "🌿"
        "dill" -> "🌿"
        "chervil" -> "🌿"
        "mint" -> "🌿"
        "thyme" -> "🌿"
        "rosemary" -> "🌿"
        "sage" -> "🌿"
        "oregano" -> "🌿"
        "tarragon" -> "🌿"
        "sorrel" -> "🌿"
        "strawberry" -> "🍓"
        "raspberry" -> "🫐"
        "blackcurrant" -> "🫐"
        "redcurrant" -> "🍒"
        "blueberry" -> "🫐"
        "apple_tree" -> "🍎"
        "pear_tree" -> "🍐"
        "cherry_tree" -> "🍒"
        "plum_tree" -> "🌳"
        "apricot_tree" -> "🍑"
        "fig_tree" -> "🌳"
        "hazelnut_tree" -> "🌰"
        "walnut_tree" -> "🌰"
        "new_zealand_spinach" -> "🌿"
        "purslane" -> "🌿"
        "artichoke" -> "🌻"
        "asparagus" -> "🌱"
        "rhubarb" -> "🌿"
        _ -> "🌱"


speciesShortName : String -> String
speciesShortName id =
    case id of
        "tomato" -> "tomate"
        "tomato_cherry" -> "tom. cer."
        "pepper" -> "poivron"
        "eggplant" -> "auberg."
        "zucchini" -> "courg."
        "butternut_squash" -> "butter."
        "pumpkin" -> "potiron"
        "cucumber" -> "concomb."
        "melon" -> "melon"
        "carrot" -> "carotte"
        "radish" -> "radis"
        "turnip" -> "navet"
        "beet" -> "bettrv."
        "parsnip" -> "panais"
        "celeriac" -> "céleri"
        "potato" -> "patate"
        "jerusalem_artichoke" -> "topinam."
        "lettuce" -> "laitue"
        "lambs_lettuce" -> "mâche"
        "spinach" -> "épinard"
        "arugula" -> "roquette"
        "chard" -> "blette"
        "kale" -> "kale"
        "cabbage" -> "chou"
        "cauliflower" -> "chou-fl."
        "broccoli" -> "brocoli"
        "brussels_sprout" -> "bruxel."
        "bean_green" -> "haric. v."
        "bean_dry" -> "haric. s."
        "pea" -> "pois"
        "broad_bean" -> "fève"
        "onion" -> "oignon"
        "shallot" -> "échalot."
        "garlic" -> "ail"
        "leek" -> "poireau"
        "chives" -> "ciboulet."
        "basil" -> "basilic"
        "parsley" -> "persil"
        "coriander" -> "coriand."
        "dill" -> "aneth"
        "chervil" -> "cerfeuil"
        "mint" -> "menthe"
        "thyme" -> "thym"
        "rosemary" -> "romarin"
        "sage" -> "sauge"
        "oregano" -> "origan"
        "tarragon" -> "estrag."
        "sorrel" -> "oseille"
        "strawberry" -> "fraise"
        "raspberry" -> "framb."
        "blackcurrant" -> "cassis"
        "redcurrant" -> "groseil."
        "blueberry" -> "myrtil."
        "apple_tree" -> "pommier"
        "pear_tree" -> "poirier"
        "cherry_tree" -> "cerisier"
        "plum_tree" -> "prunier"
        "apricot_tree" -> "abrico."
        "fig_tree" -> "figuier"
        "hazelnut_tree" -> "noiset."
        "walnut_tree" -> "noyer"
        "new_zealand_spinach" -> "tétrag."
        "purslane" -> "pourpier"
        "artichoke" -> "artich."
        "asparagus" -> "asperge"
        "rhubarb" -> "rhubarb."
        _ -> id


stateEmoji : TileState -> String
stateEmoji st =
    case st of
        TileEmpty -> ""
        TileTilled -> "🟫"
        TileSown sp -> "🌱"
        TileGrowing sp p ->
            if p < 0.4 then "🌱"
            else if p < 0.7 then "🌿"
            else speciesEmoji sp

        TileMature sp -> speciesEmoji sp
        TileHarvested sp -> "✂"


stateLabel : TileState -> String
stateLabel st =
    case st of
        TileEmpty -> "vide"
        TileTilled -> "labourée"
        TileSown sp -> "semée " ++ sp
        TileGrowing sp p -> sp ++ " " ++ String.fromInt (round (p * 100)) ++ "%"
        TileMature sp -> "prêt " ++ sp
        TileHarvested sp -> "récolté " ++ sp


stateBackground : TileState -> String
stateBackground st =
    case st of
        TileEmpty -> "#c9a66b"
        TileTilled -> "#8b6e3d"
        TileSown _ -> "#a8c379"
        TileGrowing _ _ -> "#8fbc4a"
        TileMature _ -> "#d4a033"
        TileHarvested _ -> "#a08048"


viewParcels : Model -> Html Msg
viewParcels model =
    let
        isEditing = model.editingParcel /= Nothing
        f = model.parcelForm
    in
    div [ A.class "panel" ]
        [ h2 [] [ text "Parcelles" ]
        , div [ A.style "display" "flex", A.style "flex-direction" "column", A.style "gap" "0.3rem", A.style "margin-bottom" "0.8rem" ]
            [ labeledInput "Nom" f.name SetParcelName "ex : P1 Nord"
            , labeledInput "Surface (m²)" f.surface SetParcelSurface "ex : 8.5"
            , labeledInput "Exposition" f.exposition SetParcelExposition "ex : plein sud"
            , labeledTextarea "Notes sol" f.soilNotes SetParcelSoilNotes "ex : argilo-calcaire, drainé"
            , div [ A.style "display" "grid", A.style "grid-template-columns" "1fr 1fr 1fr 1fr", A.style "gap" "0.3rem" ]
                [ labeledInput "Grille X" f.gridX SetParcelGridX "0"
                , labeledInput "Grille Y" f.gridY SetParcelGridY "0"
                , labeledInput "Largeur" f.gridW SetParcelGridW "2"
                , labeledInput "Hauteur" f.gridH SetParcelGridH "2"
                ]
            , div [ A.style "display" "flex", A.style "gap" "0.3rem", A.style "align-items" "center" ]
                [ Html.label [ A.style "font-size" "0.78rem", A.style "color" "#5a3a22" ] [ text "Couleur" ]
                , input [ A.type_ "color", A.value f.color, E.onInput SetParcelColor ] []
                ]
            , div [ A.class "controls" ]
                [ button [ E.onClick SubmitParcel, A.class "primary" ]
                    [ text (if isEditing then "✓ Mettre à jour" else "+ Ajouter parcelle") ]
                , if isEditing then
                    button [ E.onClick CancelEditParcel ] [ text "Annuler" ]
                  else text ""
                ]
            ]
        , if List.isEmpty model.parcels then
            p [ A.style "color" "#5a3a22", A.style "font-size" "0.85rem" ]
                [ text "Aucune parcelle. Crée-en une pour démarrer." ]
          else
            div [] (List.map viewParcelRow model.parcels)
        ]


viewParcelRow : Parcel -> Html Msg
viewParcelRow p =
    div [ A.class "pantry-row", A.style "padding" "0.5rem 0", A.style "border-bottom" "1px solid #e2d2a8" ]
        [ div [ A.style "flex" "1" ]
            [ div [] [ Html.strong [] [ text p.name ] ]
            , div [ A.style "font-size" "0.76rem", A.style "color" "#5a3a22" ]
                [ text
                    (String.join " · "
                        (List.filterMap identity
                            [ p.surfaceM2 |> Maybe.map (\s -> String.fromFloat s ++ " m²")
                            , p.exposition
                            , p.soilNotes
                            ]
                        )
                    )
                ]
            ]
        , div [ A.style "display" "flex", A.style "gap" "0.3rem" ]
            [ button [ E.onClick (EditParcel p), A.style "padding" "3px 6px", A.style "font-size" "0.75rem" ] [ text "✎" ]
            , button [ E.onClick (DeleteParcel p.id), A.class "danger", A.style "padding" "3px 6px", A.style "font-size" "0.75rem" ] [ text "🗑" ]
            ]
        ]


viewActionForm : Model -> Html Msg
viewActionForm model =
    let
        isEditing = model.editingAction /= Nothing
        f = model.actionForm
        tileLabel =
            if f.gridX == "" || f.gridY == "" then
                "Aucune tuile sélectionnée — clique sur la grille ci-dessus"
            else
                "Tuile (" ++ f.gridX ++ ", " ++ f.gridY ++ ")"
        speciesOptions = speciesOptionsForForm model
    in
    div [ A.class "panel" ]
        [ h2 [] [ text (if isEditing then "Modifier action" else "+ Nouvelle action") ]
        , p [ A.style "font-size" "0.82rem", A.style "color" "#5a3a22", A.style "margin-bottom" "0.5rem" ]
            [ text tileLabel ]
        , div [ A.style "display" "grid", A.style "grid-template-columns" "1fr 1fr", A.style "gap" "0.4rem" ]
            [ labeledInput "Date" f.date SetActionDate "2026-04-24"
            , labeledSelect "Type" f.kind SetActionKind (List.map (\k -> ( k, actionKindLabel k )) model.actionKinds)
            , labeledSelect "Espèce" f.speciesId SetActionSpecies speciesOptions
            , labeledInput "Quantité (g)" f.quantity SetActionQty "ex : 1500 (pour une récolte)"
            , labeledTextarea "Notes" f.notes SetActionNotes "observations, variété..."
            ]
        , div [ A.class "controls", A.style "margin-top" "0.5rem" ]
            [ button
                [ E.onClick SubmitAction
                , A.class "primary"
                , A.disabled (f.gridX == "" || f.gridY == "")
                ]
                [ text (if isEditing then "✓ Mettre à jour" else "+ Enregistrer") ]
            , if isEditing then
                button [ E.onClick CancelEditAction ] [ text "Annuler" ]
              else text ""
            ]
        ]


speciesOptionsForForm : Model -> List ( String, String )
speciesOptionsForForm model =
    case model.calendar of
        Nothing -> [ ( "", "— (catalogue non chargé) —" ) ]
        Just cal ->
            let
                doy = isoToDoy (effectiveToday model)
                isRecommended sl =
                    (case sl.indoorSowLocal of
                        Just w -> doyInWindow doy w
                        Nothing -> False
                    )
                        || (case sl.directSowLocal of
                                Just w -> doyInWindow doy w
                                Nothing -> False
                           )
                        || doyInWindow doy sl.harvestLocal

                ( recommended, other ) = List.partition isRecommended cal.species

                toOpt prefix sl =
                    ( sl.species.id, prefix ++ speciesEmoji sl.species.id ++ " " ++ sl.species.nameFr )
            in
            ( ( "", "— choisir une espèce —" )
                :: List.map (toOpt "⭐ ") recommended
            )
                ++ List.map (toOpt "") other


viewActionsTimeline : Model -> Html Msg
viewActionsTimeline model =
    let
        filtered =
            model.actions
                |> List.filter
                    (\a ->
                        (case model.filterActionParcel of
                            Just pid -> a.parcelId == Just pid
                            Nothing -> True
                        )
                            && (case model.filterActionKind of
                                    Just k -> a.kind == k
                                    Nothing -> True
                               )
                    )
    in
    div [ A.class "panel" ]
        [ h2 [] [ text ("Journal (" ++ String.fromInt (List.length filtered) ++ " / " ++ String.fromInt (List.length model.actions) ++ ")") ]
        , div [ A.style "display" "flex", A.style "gap" "0.6rem", A.style "flex-wrap" "wrap", A.style "margin-bottom" "0.6rem" ]
            [ div [ A.style "display" "flex", A.style "flex-direction" "column" ]
                [ Html.label [ A.style "font-size" "0.78rem", A.style "color" "#5a3a22" ] [ text "Parcelle" ]
                , select
                    [ E.onInput SetFilterActionParcel
                    , A.style "padding" "4px", A.style "border" "1px solid #d4b85a"
                    , A.style "border-radius" "3px", A.style "background" "#fff6de"
                    ]
                    (option [ A.value "" ] [ text "(toutes)" ]
                        :: List.map (\p -> option [ A.value (String.fromInt p.id) ] [ text p.name ]) model.parcels
                    )
                ]
            , div [ A.style "display" "flex", A.style "flex-direction" "column" ]
                [ Html.label [ A.style "font-size" "0.78rem", A.style "color" "#5a3a22" ] [ text "Type" ]
                , select
                    [ E.onInput SetFilterActionKind
                    , A.style "padding" "4px", A.style "border" "1px solid #d4b85a"
                    , A.style "border-radius" "3px", A.style "background" "#fff6de"
                    ]
                    (option [ A.value "" ] [ text "(tous)" ]
                        :: List.map (\k -> option [ A.value k ] [ text (actionKindLabel k) ]) model.actionKinds
                    )
                ]
            ]
        , if List.isEmpty filtered then
            p [ A.style "color" "#5a3a22", A.style "font-size" "0.85rem" ]
                [ text "Aucune action enregistrée. Ajoute-en une." ]
          else
            div [] (List.map (viewActionRow model) filtered)
        ]


viewActionRow : Model -> ActionEntry -> Html Msg
viewActionRow _ a =
    let
        tileLabel =
            case ( a.gridX, a.gridY ) of
                ( Just c, Just r ) -> "(" ++ String.fromInt c ++ "," ++ String.fromInt r ++ ")"
                _ -> "—"
    in
    div [ A.class "pantry-row", A.style "padding" "0.5rem 0", A.style "border-bottom" "1px solid #e2d2a8", A.style "flex-direction" "column", A.style "align-items" "stretch" ]
        [ div [ A.style "display" "flex", A.style "justify-content" "space-between" ]
            [ div []
                [ span [ A.style "color" "#8b6e3d", A.style "font-size" "0.8rem", A.style "margin-right" "0.4rem" ] [ text a.date ]
                , span [ A.class "kind-badge", A.style "background" (kindColor a.kind), A.style "color" "white"
                        , A.style "padding" "2px 6px", A.style "border-radius" "3px", A.style "font-size" "0.72rem"
                        ]
                    [ text (actionKindLabel a.kind) ]
                , span [ A.style "margin-left" "0.4rem", A.style "color" "#8b6e3d", A.style "font-size" "0.78rem" ] [ text tileLabel ]
                , case a.speciesId of
                    Just sp -> span [ A.style "margin-left" "0.4rem" ] [ text (speciesEmoji sp ++ " " ++ sp) ]
                    Nothing -> text ""
                , case a.quantityG of
                    Just g -> span [ A.style "margin-left" "0.4rem" ] [ text ("· " ++ formatQty g) ]
                    Nothing -> text ""
                ]
            , div [ A.style "display" "flex", A.style "gap" "0.3rem" ]
                [ button [ E.onClick (EditAction a), A.style "padding" "3px 6px", A.style "font-size" "0.75rem" ] [ text "✎" ]
                , button [ E.onClick (DeleteAction a.id), A.class "danger", A.style "padding" "3px 6px", A.style "font-size" "0.75rem" ] [ text "🗑" ]
                ]
            ]
        , case a.notes of
            Just n -> div [ A.style "font-size" "0.82rem", A.style "color" "#5a3a22", A.style "margin-top" "0.2rem" ] [ text n ]
            Nothing -> text ""
        ]


formatQty : Float -> String
formatQty g =
    if g >= 1000 then String.fromFloat (round1 (g / 1000)) ++ " kg"
    else String.fromFloat g ++ " g"


actionKindLabel : String -> String
actionKindLabel k =
    case k of
        "semis_direct" -> "semis direct"
        "semis_abri" -> "semis sous abri"
        "repiquage" -> "repiquage"
        "arrosage" -> "arrosage"
        "paillage" -> "paillage"
        "compost" -> "compost"
        "recolte" -> "récolte"
        "traitement" -> "traitement"
        "arrachage" -> "arrachage"
        "note" -> "note"
        _ -> k


kindColor : String -> String
kindColor k =
    case k of
        "semis_direct" -> "#6b9c47"
        "semis_abri" -> "#c66339"
        "repiquage" -> "#5a8ab8"
        "arrosage" -> "#5a9bc5"
        "paillage" -> "#8b6e3d"
        "compost" -> "#6b5030"
        "recolte" -> "#d4a033"
        "traitement" -> "#a05a5a"
        "arrachage" -> "#8b4a2a"
        "note" -> "#8b6e3d"
        _ -> "#888"


labeledInput : String -> String -> (String -> Msg) -> String -> Html Msg
labeledInput label value toMsg placeholder =
    div [ A.style "display" "flex", A.style "flex-direction" "column" ]
        [ Html.label [ A.style "font-size" "0.78rem", A.style "color" "#5a3a22" ] [ text label ]
        , input
            [ A.type_ "text", A.value value, E.onInput toMsg, A.placeholder placeholder
            , A.style "padding" "4px", A.style "border" "1px solid #d4b85a"
            , A.style "border-radius" "3px", A.style "background" "#fff6de"
            ]
            []
        ]


labeledTextarea : String -> String -> (String -> Msg) -> String -> Html Msg
labeledTextarea label value toMsg placeholder =
    div [ A.style "display" "flex", A.style "flex-direction" "column" ]
        [ Html.label [ A.style "font-size" "0.78rem", A.style "color" "#5a3a22" ] [ text label ]
        , textarea
            [ A.value value, E.onInput toMsg, A.placeholder placeholder, A.rows 2
            , A.style "padding" "4px", A.style "border" "1px solid #d4b85a"
            , A.style "border-radius" "3px", A.style "background" "#fff6de"
            , A.style "font-family" "inherit"
            ]
            []
        ]


labeledSelect : String -> String -> (String -> Msg) -> List ( String, String ) -> Html Msg
labeledSelect label value toMsg options =
    div [ A.style "display" "flex", A.style "flex-direction" "column" ]
        [ Html.label [ A.style "font-size" "0.78rem", A.style "color" "#5a3a22" ] [ text label ]
        , select
            [ E.onInput toMsg
            , A.style "padding" "4px", A.style "border" "1px solid #d4b85a"
            , A.style "border-radius" "3px", A.style "background" "#fff6de"
            ]
            (List.map (\( v, lbl ) -> option [ A.value v, A.selected (v == value) ] [ text lbl ]) options)
        ]


round1 : Float -> Float
round1 x = toFloat (round (x * 10)) / 10


isoFromDoy : Int -> Int -> String -> String
isoFromDoy _ targetDoy iso =
    case String.split "-" iso of
        [ y, _, _ ] ->
            let
                ( m, d ) = doyToMd targetDoy
                pad n = String.padLeft 2 '0' (String.fromInt n)
            in
            y ++ "-" ++ pad m ++ "-" ++ pad d
        _ -> iso
