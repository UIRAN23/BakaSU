package org.bakasu.bakasu.domain.model

data class SuSFSLkmStatus(
    val enabled: Boolean,
    val supported: Boolean,
    val available: Boolean,
    val version: String,
    val kmi: String,
    val state: String,
    val message: String,
)
