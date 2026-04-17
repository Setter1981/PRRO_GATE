using System;
using System.ComponentModel;
using System.Data.SQLite;
using Microsoft.VisualBasic;
using Microsoft.VisualBasic.CompilerServices;

namespace WebCheck;

internal class SQLrobot
{
	private string Connection;

	public SQLrobot(string ConnectionE)
	{
		Connection = ConnectionE;
	}

	internal TypErrLLCNshift ReturnLocalCheckNumberShift()
	{
		string returnStr = ReturnOpenShift().ReturnStr;
		TypErrLLCNshift result = default(TypErrLLCNshift);
		result.errCode = 0;
		result.errStr = "";
		result.LastLocalCheckNumbern = "0";
		result.IDshift = "0";
		result.Cashier = "";
		result.OperatorID = "";
		try
		{
			SQLiteConnection sQLiteConnection = new SQLiteConnection();
			new SQLiteCommand();
			sQLiteConnection.ConnectionString = Connection;
			sQLiteConnection.Open();
			SQLiteCommand sQLiteCommand = sQLiteConnection.CreateCommand();
			sQLiteCommand.CommandText = "SELECT ID, LastLocalCheckNumber, ONAME, OPERATORID FROM SHIFTS WHERE ID = '" + returnStr + "'";
			SQLiteDataReader sQLiteDataReader = sQLiteCommand.ExecuteReader();
			sQLiteDataReader.Read();
			result.IDshift = sQLiteDataReader[0].ToString();
			result.LastLocalCheckNumbern = sQLiteDataReader[1].ToString();
			result.Cashier = sQLiteDataReader[2].ToString();
			result.OperatorID = sQLiteDataReader[3].ToString();
			((Component)(object)sQLiteCommand).Dispose();
			((Component)(object)sQLiteCommand).Dispose();
			sQLiteConnection.Close();
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			result.IDshift = "-1";
			result.LastLocalCheckNumbern = "-1";
			result.errStr = "Не могу получить текущий локальный номер чека из таблицы SHIFTS";
			result.errCode = 15;
			ProjectData.ClearProjectError();
		}
		if (!Versioned.IsNumeric((object)result.LastLocalCheckNumbern))
		{
			result.IDshift = "-1";
			result.LastLocalCheckNumbern = "-1";
			result.errStr = "Локальный номер чека не является числом - талица SHIFTS";
			result.errCode = 15;
		}
		return result;
	}

	public TypErrOperKeyPass OperatorKeyPass(string idShiftS)
	{
		TypErrOperKeyPass result = default(TypErrOperKeyPass);
		result.errCode = 0;
		result.errStr = "";
		result.KeyFile = "";
		result.Pass = "";
		TypErrStr typErrStr = INNoperatorInShift(idShiftS);
		if (typErrStr.errCode > 0)
		{
			result.errCode = typErrStr.errCode;
			result.errStr = typErrStr.errStr;
			return result;
		}
		result = OperatorInfa(typErrStr.ReturnStr);
		if (result.errCode > 0)
		{
			result.KeyFile = "";
			result.Pass = "";
		}
		return result;
	}

	internal TypErrStr INNoperatorInShift(string idShiftS)
	{
		TypErrStr result = default(TypErrStr);
		result.errCode = 0;
		result.errStr = "";
		result.ReturnStr = "";
		try
		{
			SQLiteConnection sQLiteConnection = new SQLiteConnection();
			new SQLiteCommand();
			sQLiteConnection.ConnectionString = Connection;
			sQLiteConnection.Open();
			SQLiteCommand sQLiteCommand = sQLiteConnection.CreateCommand();
			sQLiteCommand.CommandText = "SELECT * FROM SHIFTS WHERE ID='" + idShiftS + "'";
			SQLiteDataReader sQLiteDataReader = sQLiteCommand.ExecuteReader();
			sQLiteDataReader.Read();
			result.ReturnStr = sQLiteDataReader[10].ToString();
			((Component)(object)sQLiteCommand).Dispose();
			sQLiteDataReader.Close();
			sQLiteConnection.Close();
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			result.errCode = 0;
			result.errStr = "";
			result.ReturnStr = "";
			ProjectData.ClearProjectError();
		}
		return result;
	}

	internal TypErrOperKeyPass OperatorInfa(string INNop)
	{
		TypErrOperKeyPass result = default(TypErrOperKeyPass);
		result.errCode = 0;
		result.errStr = "";
		result.KeyFile = "";
		result.Pass = "";
		try
		{
			SQLiteConnection sQLiteConnection = new SQLiteConnection();
			new SQLiteCommand();
			sQLiteConnection.ConnectionString = Connection;
			sQLiteConnection.Open();
			SQLiteCommand sQLiteCommand = sQLiteConnection.CreateCommand();
			sQLiteCommand.CommandText = "SELECT * FROM OPERATORS WHERE INN=" + INNop;
			SQLiteDataReader sQLiteDataReader = sQLiteCommand.ExecuteReader();
			sQLiteDataReader.Read();
			result.KeyFile = sQLiteDataReader[2].ToString();
			Coding coding = new Coding();
			result.Pass = coding.DeCod(sQLiteDataReader[3].ToString());
			((Component)(object)sQLiteCommand).Dispose();
			sQLiteDataReader.Close();
			sQLiteConnection.Close();
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			result.errCode = 5;
			result.errStr = "Немає такого оператора.";
			result.KeyFile = "";
			result.Pass = "";
			ProjectData.ClearProjectError();
		}
		return result;
	}

	internal TypErrStr ReturnOpenShift()
	{
		TypErrStr result = default(TypErrStr);
		result.errCode = 0;
		result.errStr = "";
		result.ReturnStr = "0";
		try
		{
			SQLiteConnection sQLiteConnection = new SQLiteConnection();
			new SQLiteCommand();
			sQLiteConnection.ConnectionString = Connection;
			sQLiteConnection.Open();
			SQLiteCommand sQLiteCommand = sQLiteConnection.CreateCommand();
			sQLiteCommand.CommandText = "SELECT ID FROM SHIFTS WHERE DATEEND = 'NULL'";
			SQLiteDataReader sQLiteDataReader = sQLiteCommand.ExecuteReader();
			sQLiteDataReader.Read();
			result.ReturnStr = sQLiteDataReader[0].ToString();
			((Component)(object)sQLiteCommand).Dispose();
			((Component)(object)sQLiteCommand).Dispose();
			sQLiteConnection.Close();
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			result.ReturnStr = "0";
			ProjectData.ClearProjectError();
		}
		if (Operators.CompareString(result.ReturnStr, "", false) == 0)
		{
			result.ReturnStr = "0";
		}
		if (Operators.CompareString(result.ReturnStr, "0", false) != 0 && Operators.CompareString(result.ReturnStr, MaxID("SHIFTS").ReturnStr, false) != 0)
		{
			result.ReturnStr = "-1";
			result.errCode = 1003;
			result.errStr = "ВНИМАНИЕ! Открыто несколько смен!!!";
		}
		if (Operators.CompareString(result.ReturnStr, "0", false) == 0)
		{
			result.ReturnStr = MaxID("SHIFTS").ReturnStr;
		}
		return result;
	}

	internal TypErrStr MaxID(string TAB)
	{
		TypErrStr result = default(TypErrStr);
		result.errCode = 0;
		result.errStr = "";
		result.ReturnStr = "0";
		try
		{
			SQLiteConnection sQLiteConnection = new SQLiteConnection();
			new SQLiteCommand();
			sQLiteConnection.ConnectionString = Connection;
			sQLiteConnection.Open();
			SQLiteCommand sQLiteCommand = sQLiteConnection.CreateCommand();
			sQLiteCommand.CommandText = "SELECT MAX(ID) FROM " + TAB;
			SQLiteDataReader sQLiteDataReader = sQLiteCommand.ExecuteReader();
			sQLiteDataReader.Read();
			result.ReturnStr = sQLiteDataReader[0].ToString();
			((Component)(object)sQLiteCommand).Dispose();
			((Component)(object)sQLiteCommand).Dispose();
			sQLiteConnection.Close();
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			result.ReturnStr = "0";
			ProjectData.ClearProjectError();
		}
		if (Operators.CompareString(result.ReturnStr, "", false) == 0)
		{
			result.ReturnStr = "0";
		}
		return result;
	}

	internal int OfflineOpenID()
	{
		string text = "0";
		try
		{
			SQLiteConnection sQLiteConnection = new SQLiteConnection();
			new SQLiteCommand();
			sQLiteConnection.ConnectionString = Connection;
			sQLiteConnection.Open();
			SQLiteCommand sQLiteCommand = sQLiteConnection.CreateCommand();
			sQLiteCommand.CommandText = "Select ID FROM ksef WHERE offline = '3'";
			SQLiteDataReader sQLiteDataReader = sQLiteCommand.ExecuteReader();
			text = ((!sQLiteDataReader.Read()) ? "0" : sQLiteDataReader[0].ToString());
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

	internal TypErrKsef CheckForSend()
	{
		TypErrKsef result = default(TypErrKsef);
		result.errCode = 0;
		result.errStr = "";
		result.ReturnXML = "";
		result.ReturnID = "";
		result.ReturnLocalNumber = "";
		result.ReturnShift = "";
		result.ReturnNumber = "";
		result.ReturnTyp = "";
		try
		{
			SQLiteConnection sQLiteConnection = new SQLiteConnection();
			new SQLiteCommand();
			sQLiteConnection.ConnectionString = Connection;
			sQLiteConnection.Open();
			SQLiteCommand sQLiteCommand = sQLiteConnection.CreateCommand();
			sQLiteCommand.CommandText = "SELECT * FROM ksef WHERE offline='2'";
			SQLiteDataReader sQLiteDataReader = sQLiteCommand.ExecuteReader();
			if (sQLiteDataReader.Read())
			{
				result.ReturnXML = sQLiteDataReader[2].ToString();
				result.ReturnID = sQLiteDataReader[11].ToString();
				result.ReturnLocalNumber = sQLiteDataReader[5].ToString();
				result.ReturnShift = sQLiteDataReader[9].ToString();
				result.ReturnNumber = sQLiteDataReader[4].ToString();
				result.ReturnTyp = sQLiteDataReader[6].ToString();
			}
			else
			{
				result.errCode = 38;
				result.errStr = "Ошибка чтения данных из таблицы ksef";
			}
			((Component)(object)sQLiteCommand).Dispose();
			((Component)(object)sQLiteCommand).Dispose();
			sQLiteConnection.Close();
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			result.errCode = 38;
			result.errStr = "Ошибка получение данных из таблицы ksef";
			ProjectData.ClearProjectError();
		}
		return result;
	}

	internal TypErrKsef LastCheckForSend()
	{
		TypErrKsef result = default(TypErrKsef);
		result.errCode = 0;
		result.errStr = "";
		result.ReturnXML = "";
		result.ReturnID = "";
		result.ReturnLocalNumber = "";
		result.ReturnShift = "";
		result.ReturnNumber = "";
		result.ReturnTyp = "";
		string returnStr = MaxID("ksef").ReturnStr;
		try
		{
			SQLiteConnection sQLiteConnection = new SQLiteConnection();
			new SQLiteCommand();
			sQLiteConnection.ConnectionString = Connection;
			sQLiteConnection.Open();
			SQLiteCommand sQLiteCommand = sQLiteConnection.CreateCommand();
			sQLiteCommand.CommandText = "SELECT * FROM ksef WHERE ID='" + returnStr + "'";
			SQLiteDataReader sQLiteDataReader = sQLiteCommand.ExecuteReader();
			if (sQLiteDataReader.Read())
			{
				result.ReturnXML = sQLiteDataReader[2].ToString();
				result.ReturnID = sQLiteDataReader[11].ToString();
				result.ReturnLocalNumber = sQLiteDataReader[5].ToString();
				result.ReturnShift = sQLiteDataReader[9].ToString();
				result.ReturnNumber = sQLiteDataReader[4].ToString();
				result.ReturnTyp = sQLiteDataReader[6].ToString();
			}
			else
			{
				result.errCode = 38;
				result.errStr = "Ошибка чтения данных из таблицы ksef";
			}
			((Component)(object)sQLiteCommand).Dispose();
			((Component)(object)sQLiteCommand).Dispose();
			sQLiteConnection.Close();
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			result.errCode = 38;
			result.errStr = "Ошибка получение данных из таблицы ksef";
			ProjectData.ClearProjectError();
		}
		return result;
	}

	internal TypErr UPDATEksef(string ID, string NameCol, string Infa)
	{
		TypErr result = default(TypErr);
		result.errCode = 0;
		result.errStr = "";
		Infa = Strings.Replace(Infa, "'", "\"", 1, -1, (CompareMethod)0);
		int num = 0;
		do
		{
			result.errCode = 0;
			result.errStr = "";
			try
			{
				SQLiteConnection sQLiteConnection = new SQLiteConnection();
				SQLiteCommand sQLiteCommand = new SQLiteCommand();
				sQLiteConnection.ConnectionString = Connection;
				sQLiteConnection.Open();
				sQLiteCommand = sQLiteConnection.CreateCommand();
				sQLiteCommand.CommandText = "UPDATE ksef SET " + NameCol + "='" + Infa + "' WHERE ID='" + ID.ToString() + "'";
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
				goto IL_00e3;
			}
			break;
			IL_00e3:
			num = checked(num + 1);
		}
		while (num <= 18);
		return result;
	}

	internal TypErr CloseOfflineKsef()
	{
		TypErr result = default(TypErr);
		result.errCode = 0;
		result.errStr = "";
		int num = 0;
		do
		{
			result.errCode = 0;
			result.errStr = "";
			try
			{
				SQLiteConnection sQLiteConnection = new SQLiteConnection();
				new SQLiteCommand();
				sQLiteConnection.ConnectionString = Connection;
				sQLiteConnection.Open();
				SQLiteCommand sQLiteCommand = sQLiteConnection.CreateCommand();
				sQLiteCommand.CommandText = "UPDATE ksef SET offline='1' WHERE offline='3'";
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
				goto IL_0094;
			}
			break;
			IL_0094:
			num = checked(num + 1);
		}
		while (num <= 18);
		return result;
	}

	internal TypErr SaveXMLcheck(string checkid, string checkxml, string checkXMLnotDot, string signedanswerfromficscal, string checkidficscal, string TypDoc, string SumT = "0.00", string PathFile = "")
	{
		TypErr result = default(TypErr);
		result.errCode = 0;
		result.errStr = "";
		checkxml = Strings.Replace(checkxml, "'", "\"", 1, -1, (CompareMethod)0);
		checkXMLnotDot = Strings.Replace(checkXMLnotDot, "'", "\"", 1, -1, (CompareMethod)0);
		checkidficscal = checkidficscal.Replace("`", "_");
		if (Operators.CompareString(TypDoc.Trim(), "80", false) != 0)
		{
			signedanswerfromficscal = "";
		}
		signedanswerfromficscal = Strings.Replace(signedanswerfromficscal, "'", "\"", 1, -1, (CompareMethod)0);
		TypErrStr typErrStr = ReturnOpenShift();
		string text = SHA.GenerateSHA256File(PathFile);
		string text2 = "0";
		checked
		{
			if (Operators.CompareString(TypDoc, "8", false) == 0)
			{
				text2 = "0";
				checkid = typErrStr.ReturnStr + "A";
			}
			else
			{
				TypErrLLCNshift typErrLLCNshift = ReturnLocalCheckNumberShift();
				if (typErrLLCNshift.errCode > 0)
				{
					typErrLLCNshift.LastLocalCheckNumbern = "0";
				}
				text2 = (Conversions.ToInteger(typErrLLCNshift.LastLocalCheckNumbern) + 1).ToString();
			}
			int num = Conversions.ToInteger(MaxID("ksef").ReturnStr);
			int num2 = 0;
			do
			{
				result.errCode = 0;
				result.errStr = "";
				try
				{
					SQLiteConnection sQLiteConnection = new SQLiteConnection();
					SQLiteCommand sQLiteCommand = new SQLiteCommand();
					sQLiteConnection.ConnectionString = Connection;
					sQLiteConnection.Open();
					sQLiteCommand = sQLiteConnection.CreateCommand();
					sQLiteCommand.CommandText = "INSERT INTO ksef \r\n            (checkid, checkxml, checksigned, signedanswerfromficscal, checkidficscal, shiftid, DocType, MAC, sum, dt, localchecknumber)\r\n            VALUES \r\n            ('" + checkid + "','" + checkxml + "','" + checkXMLnotDot + "','" + signedanswerfromficscal + "','" + checkidficscal + "', '" + typErrStr.ReturnStr + " ', '" + TypDoc + "', '" + text + "', '" + SumT + "', datetime(CURRENT_TIMESTAMP, 'localtime'), '" + text2 + "' )";
					SQLiteDataReader sQLiteDataReader = sQLiteCommand.ExecuteReader();
					((Component)(object)sQLiteCommand).Dispose();
					sQLiteDataReader.Close();
					sQLiteConnection.Close();
				}
				catch (Exception ex)
				{
					ProjectData.SetProjectError(ex);
					Exception ex2 = ex;
					if (num + 1 == Conversions.ToInteger(MaxID("ksef").ReturnStr))
					{
						ProjectData.ClearProjectError();
						break;
					}
					result.errCode = 16;
					result.errStr = "Ошибка записи информации о чеке в таблицу ksef";
					ProjectData.ClearProjectError();
					goto IL_025a;
				}
				break;
				IL_025a:
				num2++;
			}
			while (num2 <= 18);
			return result;
		}
	}

	internal void LastOfflineDate()
	{
		string returnStr = MaxID("ksef").ReturnStr;
		if (Operators.CompareString(returnStr, "0", false) == 0)
		{
			return;
		}
		TypErrKsefLast typErrKsefLast = LastCheckKsef();
		if (typErrKsefLast.errCode <= 0)
		{
			DateTime dateTime = Convert.ToDateTime(typErrKsefLast.ReturnDate);
			if (Math.Abs(DateAndTime.DateDiff((DateInterval)8, DateTime.Now, dateTime, (FirstDayOfWeek)1, (FirstWeekOfYear)1)) >= 60 && Operators.CompareString(returnStr, typErrKsefLast.ReturnID, false) == 0 && ((Operators.CompareString(typErrKsefLast.ReturnTyp, "9", false) == 0) & (Operators.CompareString(typErrKsefLast.ReturnOffline, "2", false) == 0)))
			{
				UpdateTypeksef(returnStr);
			}
		}
	}

	private TypErrKsefLast LastCheckKsef()
	{
		TypErrKsefLast result = default(TypErrKsefLast);
		result.errCode = 0;
		result.errStr = "";
		result.ReturnID = "";
		result.ReturnShift = "";
		result.ReturnTyp = "";
		result.ReturnOffline = "";
		result.ReturnDate = "";
		string returnStr = MaxID("ksef").ReturnStr;
		try
		{
			SQLiteConnection sQLiteConnection = new SQLiteConnection();
			new SQLiteCommand();
			sQLiteConnection.ConnectionString = Connection;
			sQLiteConnection.Open();
			SQLiteCommand sQLiteCommand = sQLiteConnection.CreateCommand();
			sQLiteCommand.CommandText = "SELECT * FROM ksef WHERE ID='" + returnStr + "'";
			SQLiteDataReader sQLiteDataReader = sQLiteCommand.ExecuteReader();
			if (sQLiteDataReader.Read())
			{
				result.ReturnID = sQLiteDataReader[11].ToString().Trim();
				result.ReturnShift = sQLiteDataReader[9].ToString().Trim();
				result.ReturnTyp = sQLiteDataReader[6].ToString().Trim();
				result.ReturnOffline = sQLiteDataReader[12].ToString().Trim();
				result.ReturnDate = sQLiteDataReader[10].ToString().Trim();
			}
			else
			{
				result.errCode = 38;
				result.errStr = "Ошибка чтения данных из таблицы ksef";
			}
			((Component)(object)sQLiteCommand).Dispose();
			((Component)(object)sQLiteCommand).Dispose();
			sQLiteConnection.Close();
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			result.errCode = 38;
			result.errStr = "Ошибка получение данных из таблицы ksef";
			ProjectData.ClearProjectError();
		}
		return result;
	}

	private TypErr UpdateDateksef(string id)
	{
		TypErr result = default(TypErr);
		result.errCode = 0;
		result.errStr = "";
		try
		{
			SQLiteConnection sQLiteConnection = new SQLiteConnection();
			new SQLiteCommand();
			sQLiteConnection.ConnectionString = Connection;
			sQLiteConnection.Open();
			SQLiteCommand sQLiteCommand = sQLiteConnection.CreateCommand();
			sQLiteCommand.CommandText = "UPDATE ksef SET dt = datetime(CURRENT_TIMESTAMP, 'localtime') WHERE ID='" + id + "'";
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

	private TypErr UpdateTypeksef(string id)
	{
		TypErr result = default(TypErr);
		result.errCode = 0;
		result.errStr = "";
		try
		{
			SQLiteConnection sQLiteConnection = new SQLiteConnection();
			new SQLiteCommand();
			sQLiteConnection.ConnectionString = Connection;
			sQLiteConnection.Open();
			SQLiteCommand sQLiteCommand = sQLiteConnection.CreateCommand();
			sQLiteCommand.CommandText = "UPDATE ksef SET offline = '-1', checkidficscal = 'webcheck' WHERE ID='" + id + "'";
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

	internal TypErrStr IDksefType3()
	{
		TypErrStr result = default(TypErrStr);
		result.ReturnStr = "0";
		result.errCode = 0;
		result.errStr = "";
		try
		{
			SQLiteConnection sQLiteConnection = new SQLiteConnection();
			new SQLiteCommand();
			sQLiteConnection.ConnectionString = Connection;
			sQLiteConnection.Open();
			SQLiteCommand sQLiteCommand = sQLiteConnection.CreateCommand();
			sQLiteCommand.CommandText = "Select id  from ksef where (DocType = 9 and offline=1) ORDER by ID DESC  LIMIT 1";
			SQLiteDataReader sQLiteDataReader = sQLiteCommand.ExecuteReader();
			if (sQLiteDataReader.Read())
			{
				result.ReturnStr = sQLiteDataReader[0].ToString();
			}
			else
			{
				result.errCode = 38;
				result.errStr = "Ошибка получения ID чека с типом 3 в таблице ksef";
				result.ReturnStr = "0";
			}
			((Component)(object)sQLiteCommand).Dispose();
			sQLiteDataReader.Close();
			sQLiteConnection.Close();
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			result.errStr = "Ошибка поиска чека с типом 3 в таблице ksef";
			result.errCode = 38;
			result.ReturnStr = "0";
			ProjectData.ClearProjectError();
		}
		return result;
	}
}
