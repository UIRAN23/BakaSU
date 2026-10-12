package org.bakasu.bakasu.data.susfs

import android.util.Log
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import org.bakasu.bakasu.data.shell.KsuCliRepository
import org.bakasu.bakasu.domain.model.SuSFSLkmStatus
import org.json.JSONObject

class SuSFSLkmRepository(private val ksuCliRepository: KsuCliRepository) {
    suspend fun getStatus(): SuSFSLkmStatus? = try {
        val status = JSONObject(execute("status"))
        SuSFSLkmStatus(
            enabled = status.getBoolean("enabled"),
            supported = status.getBoolean("supported"),
            available = status.getBoolean("available"),
            version = status.getString("version"),
            kmi = status.getString("kmi"),
            state = status.getString("state"),
            message = status.getString("message"),
        )
    } catch (e: CancellationException) {
        throw e
    } catch (e: Exception) {
        Log.e("SuSFSLkm", "Failed to load status", e)
        null
    }

    suspend fun setEnabled(enabled: Boolean): Boolean = try {
        execute(if (enabled) "enable" else "disable")
        true
    } catch (e: CancellationException) {
        throw e
    } catch (e: Exception) {
        Log.e("SuSFSLkm", "Failed to update autoload setting", e)
        false
    }

    private suspend fun execute(command: String): String = withContext(Dispatchers.IO) {
        val stdout = ArrayList<String>()
        val stderr = ArrayList<String>()
        val daemon = ksuCliRepository.getKsuDaemonPath().replace("'", "'\"'\"'")
        val result = ksuCliRepository.withNewRootShell {
            newJob()
                .add("'$daemon' susfs lkm $command")
                .to(stdout, stderr)
                .exec()
        }
        check(result.isSuccess) { stderr.joinToString("\n") }
        stdout.joinToString("\n").trim()
    }
}
