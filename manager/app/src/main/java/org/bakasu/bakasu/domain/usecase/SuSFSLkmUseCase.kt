package org.bakasu.bakasu.domain.usecase

import org.bakasu.bakasu.data.susfs.SuSFSLkmRepository

class SuSFSLkmUseCase(private val repository: SuSFSLkmRepository) {
    suspend fun getStatus() = repository.getStatus()
    suspend fun setEnabled(enabled: Boolean) = repository.setEnabled(enabled)
}
