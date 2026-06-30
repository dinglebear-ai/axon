package com.axon.app.ui.nav

sealed interface DrawerSection {
    data object Activity   : DrawerSection
    data object Sessions   : DrawerSection
    data object Jobs       : DrawerSection
    data object Knowledge  : DrawerSection
    data object Settings   : DrawerSection
}
