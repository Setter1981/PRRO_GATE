using System;
using System.ComponentModel;
using System.Data.SQLite;
using Microsoft.VisualBasic;
using Microsoft.VisualBasic.CompilerServices;

namespace WebCheck;

internal class UpdateInfa
{
	internal TypErr UPDATE(string Table, string NameCol, string ID, string NewInfa)
	{
		TypErr result = default(TypErr);
		result.errCode = 0;
		result.errStr = "";
		NewInfa = Strings.Replace(NewInfa, "'", "\"");
		try
		{
			SQLiteConnection sQLiteConnection = new SQLiteConnection();
			SQLiteCommand sQLiteCommand = new SQLiteCommand();
			sQLiteConnection.ConnectionString = All.A.Connection;
			sQLiteConnection.Open();
			sQLiteCommand = sQLiteConnection.CreateCommand();
			sQLiteCommand.CommandText = "UPDATE " + Table + " SET " + NameCol + "='" + NewInfa + "' WHERE ID='" + ID.ToString() + "'";
			SQLiteDataReader sQLiteDataReader = sQLiteCommand.ExecuteReader();
			((Component)(object)sQLiteCommand).Dispose();
			sQLiteDataReader.Close();
			sQLiteConnection.Close();
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			result.errStr = "Ошибка при попытке изменить запись № " + ID.ToString() + " в таблице " + Table + ", колонка " + NameCol;
			result.errCode = 40;
			ProjectData.ClearProjectError();
		}
		return result;
	}

	internal bool SearchPayForms(string NameS)
	{
		if (All.PayTax.NamePayToIndex(NameS) > 0)
		{
			return true;
		}
		return false;
	}
}
