using System;
using System.ComponentModel;
using System.Data.SQLite;
using Microsoft.VisualBasic.CompilerServices;

namespace WebCheck;

internal class NumbersOfflineUse
{
	public TypErr Add(string Number, string ksefID)
	{
		TypErr result = default(TypErr);
		result.errCode = 0;
		result.errStr = "";
		try
		{
			SQLiteConnection sQLiteConnection = new SQLiteConnection();
			SQLiteCommand sQLiteCommand = new SQLiteCommand();
			sQLiteConnection.ConnectionString = All.A.Connection;
			sQLiteConnection.Open();
			sQLiteCommand = sQLiteConnection.CreateCommand();
			sQLiteCommand.CommandText = "INSERT INTO fns \r\n            (checkidfiscal, sourceid, added)\r\n            VALUES \r\n            ('" + Number + "','" + ksefID + "', datetime(CURRENT_TIMESTAMP, 'localtime'))";
			SQLiteDataReader sQLiteDataReader = sQLiteCommand.ExecuteReader();
			((Component)(object)sQLiteCommand).Dispose();
			sQLiteDataReader.Close();
			sQLiteConnection.Close();
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			result.errCode = 16;
			result.errStr = "Ошибка записи информации о чеке в таблицу CHECKTAX";
			ProjectData.ClearProjectError();
		}
		return result;
	}

	public int CountNubmers()
	{
		string text = "0";
		try
		{
			SQLiteConnection sQLiteConnection = new SQLiteConnection();
			new SQLiteCommand();
			sQLiteConnection.ConnectionString = All.A.Connection;
			sQLiteConnection.Open();
			SQLiteCommand sQLiteCommand = sQLiteConnection.CreateCommand();
			sQLiteCommand.CommandText = "Select COUNT(*) FROM fns WHERE used is NULL";
			SQLiteDataReader sQLiteDataReader = sQLiteCommand.ExecuteReader();
			if (sQLiteDataReader.Read())
			{
				text = sQLiteDataReader[0].ToString();
			}
			((Component)(object)sQLiteCommand).Dispose();
			sQLiteDataReader.Close();
			sQLiteConnection.Close();
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			text = "0";
			ProjectData.ClearProjectError();
		}
		return Conversions.ToInteger(text);
	}

	public TypErrStr OfflineID()
	{
		TypErrStr typErrStr = default(TypErrStr);
		typErrStr.errCode = 0;
		typErrStr.errStr = "";
		typErrStr.ReturnStr = "";
		TypErrStr result;
		if (Operators.CompareString(All.A.FN, "7000000512", false) == 0)
		{
			typErrStr.ReturnStr = GetDemoOfflineNumber();
			typErrStr.ReturnStr = typErrStr.ReturnStr.Replace("_", "`");
			result = typErrStr;
		}
		else
		{
			try
			{
				SQLiteConnection sQLiteConnection = new SQLiteConnection();
				SQLiteCommand sQLiteCommand = new SQLiteCommand();
				sQLiteConnection.ConnectionString = All.A.Connection;
				sQLiteConnection.Open();
				sQLiteCommand = sQLiteConnection.CreateCommand();
				if (All.A.OfflinePullKsefCheck)
				{
					sQLiteCommand.CommandText = "Select checkidfiscal FROM fns WHERE used is NULL and (checkidfiscal NOT IN (SELECT checkidficscal from ksef )) LIMIT 1";
				}
				else
				{
					sQLiteCommand.CommandText = "Select checkidfiscal FROM fns WHERE used is NULL LIMIT 1";
				}
				SQLiteDataReader sQLiteDataReader = sQLiteCommand.ExecuteReader();
				if (sQLiteDataReader.Read())
				{
					typErrStr.ReturnStr = sQLiteDataReader[0].ToString();
				}
				((Component)(object)sQLiteCommand).Dispose();
				sQLiteDataReader.Close();
				sQLiteConnection.Close();
				if (Operators.CompareString(typErrStr.ReturnStr.Trim(), "", false) == 0)
				{
					typErrStr.errCode = 35;
					typErrStr.errStr = "Помилка отримання номера, можливо, вони закінчилися.";
					result = typErrStr;
					goto IL_016a;
				}
			}
			catch (Exception ex)
			{
				ProjectData.SetProjectError(ex);
				Exception ex2 = ex;
				typErrStr.errCode = 35;
				typErrStr.errStr = "Ошибка при получении номера для офлайн документа";
				typErrStr.ReturnStr = "";
				result = typErrStr;
				ProjectData.ClearProjectError();
				goto IL_016a;
			}
			typErrStr.ReturnStr = typErrStr.ReturnStr.Replace("_", "`");
			result = typErrStr;
		}
		goto IL_016a;
		IL_016a:
		return result;
	}

	private string GetDemoOfflineNumber()
	{
		string text = "DemoVersion";
		Random random = new Random();
		text = "";
		int num = 0;
		do
		{
			int num2 = random.Next(17);
			text = ((num2 < 10) ? (text + num2) : (num2 switch
			{
				10 => text + "W", 
				11 => text + "C", 
				12 => text + "d", 
				13 => text + "v", 
				14 => text + "H", 
				15 => text + "G", 
				16 => text + "B", 
				_ => text + "_", 
			}));
			num = checked(num + 1);
		}
		while (num <= 10);
		return text;
	}

	public TypErr UPDATEfns()
	{
		TypErr result = default(TypErr);
		result.errCode = 0;
		result.errStr = "";
		try
		{
			SQLiteConnection sQLiteConnection = new SQLiteConnection();
			new SQLiteCommand();
			sQLiteConnection.ConnectionString = All.A.Connection;
			sQLiteConnection.Open();
			SQLiteCommand sQLiteCommand = sQLiteConnection.CreateCommand();
			sQLiteCommand.CommandText = "UPDATE fns SET used='2' WHERE used is NULL";
			SQLiteDataReader sQLiteDataReader = sQLiteCommand.ExecuteReader();
			((Component)(object)sQLiteCommand).Dispose();
			sQLiteDataReader.Close();
			sQLiteConnection.Close();
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			result.errStr = "Ошибка при попытке изменить запись в таблице ksef.";
			result.errCode = 40;
			ProjectData.ClearProjectError();
		}
		return result;
	}
}
