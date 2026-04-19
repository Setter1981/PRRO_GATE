using System;
using System.ComponentModel;
using System.Data.SQLite;
using Microsoft.VisualBasic.CompilerServices;

namespace WebCheck;

internal class UpdateKeys
{
	internal bool UpdateKey(string innO)
	{
		bool result;
		try
		{
			SQLiteConnection sQLiteConnection = new SQLiteConnection();
			SQLiteCommand sQLiteCommand = new SQLiteCommand();
			sQLiteConnection.ConnectionString = All.A.Connection;
			sQLiteConnection.Open();
			sQLiteCommand = sQLiteConnection.CreateCommand();
			sQLiteCommand.CommandText = "update OPERATORS set KEYPATH = (SELECT KEYPATH from operators where inn = 'R" + innO + "' limit 1),KEYPASS = (SELECT KEYPASS from operators where inn = 'R" + innO + "' limit 1) where inn = '" + innO + "' and (0<(SELECT Count(id) from OPERATORS where  inn = 'R" + innO + "'))";
			SQLiteDataReader sQLiteDataReader = sQLiteCommand.ExecuteReader();
			((Component)(object)sQLiteCommand).Dispose();
			sQLiteDataReader.Close();
			sQLiteConnection.Close();
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			result = false;
			ProjectData.ClearProjectError();
			goto IL_009e;
		}
		result = true;
		goto IL_009e;
		IL_009e:
		return result;
	}

	internal bool DelKey(string innO)
	{
		string text = "Delete From operators where INN = 'R" + innO + "'";
		bool result;
		try
		{
			SQLiteConnection sQLiteConnection = new SQLiteConnection();
			new SQLiteCommand();
			sQLiteConnection.ConnectionString = All.A.Connection;
			sQLiteConnection.Open();
			SQLiteCommand sQLiteCommand = sQLiteConnection.CreateCommand();
			sQLiteCommand.CommandText = text;
			SQLiteDataReader sQLiteDataReader = sQLiteCommand.ExecuteReader();
			((Component)(object)sQLiteCommand).Dispose();
			sQLiteDataReader.Close();
			sQLiteConnection.Close();
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			result = false;
			ProjectData.ClearProjectError();
			goto IL_0076;
		}
		All.l.BackUpLog(text);
		result = true;
		goto IL_0076;
		IL_0076:
		return result;
	}

	internal bool DelOp(string innO)
	{
		string text = "Delete From operators where INN='" + innO + "'";
		bool result;
		try
		{
			SQLiteConnection sQLiteConnection = new SQLiteConnection();
			new SQLiteCommand();
			sQLiteConnection.ConnectionString = All.A.Connection;
			sQLiteConnection.Open();
			SQLiteCommand sQLiteCommand = sQLiteConnection.CreateCommand();
			sQLiteCommand.CommandText = text;
			SQLiteDataReader sQLiteDataReader = sQLiteCommand.ExecuteReader();
			((Component)(object)sQLiteCommand).Dispose();
			sQLiteDataReader.Close();
			sQLiteConnection.Close();
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			result = false;
			ProjectData.ClearProjectError();
			goto IL_0076;
		}
		All.l.BackUpLog(text);
		result = true;
		goto IL_0076;
		IL_0076:
		return result;
	}
}
