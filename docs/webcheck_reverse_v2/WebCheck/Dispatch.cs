using System;
using System.ComponentModel;
using System.Data.SQLite;
using System.Threading;
using System.Windows.Forms;
using Microsoft.VisualBasic;
using Microsoft.VisualBasic.CompilerServices;

namespace WebCheck;

internal class Dispatch
{
	public readonly string FN;

	public readonly string Connection;

	private readonly string TIN;

	private readonly string KeyPath;

	private readonly string Pass;

	private readonly int SerN;

	private readonly string FiscalWWW;

	private IniHGB Blok;

	private long NumberBlock;

	private readonly LogSaveTextRobot Log;

	public Dispatch(string FNe, string ConnectionE, string KeyPathE, string PassE, int SerNe, string LogPathe, string FiscalWWWe, string TinE)
	{
		NumberBlock = 0L;
		NumberBlock = 0L;
		TIN = TinE;
		FN = FNe;
		Connection = ConnectionE;
		KeyPath = KeyPathE;
		Pass = PassE;
		SerN = SerNe;
		FiscalWWW = FiscalWWWe;
		Log = new LogSaveTextRobot(Connection, FN);
		Log.PathFile = LogPathe;
		Log.FNlog = FN;
		Blok = new IniHGB(All.MyDoc() + "\\WebCheck\\Temp\\" + FN + "\\bl.ini");
		Blok.IntigerWriteFN(FN, "Block", 0);
	}

	internal void OreratorKeyDataEndIni()
	{
		new OperatorsAllRobot(Connection, FN);
	}

	internal int OfflineToOnline()
	{
		SendingOfflineChecksRobot sendingOfflineChecksRobot = new SendingOfflineChecksRobot(Connection, KeyPath, Pass, FN, FiscalWWW, TIN);
		if (OfflineCheckCount() > 0)
		{
			if (!BlockBase(bl: true))
			{
				return -2;
			}
			TypErrStr typErrStr = sendingOfflineChecksRobot.SendDoc();
			BlockBase(bl: false);
			if (typErrStr.errCode > 0)
			{
				if (typErrStr.errCode == 67)
				{
					All.f.StringWriteFN(FN, "LastOfflineErr", typErrStr.errStr);
					Log.SaveTextToLog("OfflineToOnline", typErrStr.errStr, typErrStr.ReturnStr);
					return -3;
				}
				All.f.StringWriteFN(FN, "LastOfflineErr", typErrStr.errStr);
				Log.SaveTextToLog("OfflineToOnline", typErrStr.errStr, typErrStr.ReturnStr);
				return -2;
			}
			All.f.StringWriteFN(FN, "LastOfflineErr", "");
			return 1;
		}
		if (Blok.IntegerGetFn(FN, "Block") == 0)
		{
			Blok.IntigerWriteFN(FN, "Block", 1);
			if (!BlockBase(bl: true))
			{
				Blok.IntigerWriteFN(FN, "Block", 0);
				return -1;
			}
			Thread.Sleep(999);
			if (OfflineCheckCount() > 0)
			{
				BlockBase(bl: false);
				Blok.IntigerWriteFN(FN, "Block", 0);
				Log.SaveTextToLog("OfflineToOnline", "При закрытия офлан режима в базе появился новый офлайн документ - операция прервана.");
				return -1;
			}
			TypErrStr typErrStr2 = sendingOfflineChecksRobot.CloseOfflineDoc();
			BlockBase(bl: false);
			Blok.IntigerWriteFN(FN, "Block", 0);
			if (typErrStr2.errCode > 0)
			{
				if (typErrStr2.errCode == 67)
				{
					All.f.StringWriteFN(FN, "LastOfflineErr", typErrStr2.errStr);
					Log.SaveTextToLog("OfflineToOnline", typErrStr2.errStr, typErrStr2.ReturnStr);
					return -3;
				}
				All.f.StringWriteFN(FN, "LastOfflineErr", typErrStr2.errStr);
				Log.SaveTextToLog("OfflineToOnline", typErrStr2.errStr, typErrStr2.ReturnStr);
				return -1;
			}
			All.f.StringWriteFN(FN, "LastOfflineErr", "");
			return 0;
		}
		return -1;
	}

	internal TypErr GetOfflineNumberAutomatic()
	{
		TypErr typErr = default(TypErr);
		typErr.errCode = 0;
		typErr.errStr = "";
		typErr = new SendingOfflineChecksRobot(Connection, KeyPath, Pass, FN, FiscalWWW, TIN).NewBackupNumbers();
		if (typErr.errCode > 0)
		{
			Log.SaveTextToLog("GetOfflineNumberAutomatic", "При отриманні резервних офлайн номерів сталася помилка № " + typErr.errCode, typErr.errStr);
		}
		return typErr;
	}

	internal bool OfflineTrue()
	{
		bool result;
		if (Operators.CompareString(FN, "7000000512", TextCompare: false) == 0)
		{
			result = true;
		}
		else
		{
			try
			{
				SQLiteConnection sQLiteConnection = new SQLiteConnection();
				new SQLiteCommand();
				sQLiteConnection.ConnectionString = Connection;
				sQLiteConnection.Open();
				SQLiteCommand sQLiteCommand = sQLiteConnection.CreateCommand();
				sQLiteCommand.CommandText = "SELECT ID FROM ksef WHERE offline > '1'";
				result = (sQLiteCommand.ExecuteReader().Read() ? true : false);
				((Component)(object)sQLiteCommand).Dispose();
				((Component)(object)sQLiteCommand).Dispose();
				sQLiteConnection.Close();
			}
			catch (Exception ex)
			{
				ProjectData.SetProjectError(ex);
				Exception ex2 = ex;
				result = false;
				ProjectData.ClearProjectError();
			}
		}
		return result;
	}

	internal int OfflineCheckCount()
	{
		string value = "0";
		try
		{
			SQLiteConnection sQLiteConnection = new SQLiteConnection();
			new SQLiteCommand();
			sQLiteConnection.ConnectionString = Connection;
			sQLiteConnection.Open();
			SQLiteCommand sQLiteCommand = sQLiteConnection.CreateCommand();
			sQLiteCommand.CommandText = "Select COUNT(*) FROM ksef WHERE offline = '2'";
			SQLiteDataReader sQLiteDataReader = sQLiteCommand.ExecuteReader();
			if (sQLiteDataReader.Read())
			{
				value = sQLiteDataReader[0].ToString();
			}
			((Component)(object)sQLiteCommand).Dispose();
			sQLiteDataReader.Close();
			sQLiteConnection.Close();
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			value = "0";
			ProjectData.ClearProjectError();
		}
		return Conversions.ToInteger(value);
	}

	~Dispatch()
	{
		BlockBase(bl: false);
		Blok.IntigerWriteFN(FN, "Block", 0);
		base.Finalize();
	}

	internal bool BlockBase(bool bl)
	{
		if (!bl)
		{
			EndMyBlock();
			return true;
		}
		int num = 1;
		TypErrInt typErrInt;
		do
		{
			typErrInt = SetBlockS();
			if (typErrInt.errCode == 0)
			{
				break;
			}
			if (typErrInt.errCode == 61)
			{
				NumberBlock = 0L;
			}
			else if (typErrInt.errCode > 0)
			{
				NumberBlock = 0L;
				return false;
			}
			num = checked(num + 1);
		}
		while (num <= 9);
		if (NumberBlock > 0)
		{
			if (typErrInt.ReturnInt == NumberBlock)
			{
				return true;
			}
			NumberBlock = typErrInt.ReturnInt;
			return true;
		}
		NumberBlock = typErrInt.ReturnInt;
		return true;
	}

	private TypErrInt SetBlockS()
	{
		TypErrInt result = default(TypErrInt);
		result.errCode = 0;
		result.errStr = "";
		result.ReturnInt = 0L;
		int num = 1;
		checked
		{
			do
			{
				result.errCode = 0;
				result.errStr = "";
				result.ReturnInt = 0L;
				TypErrBlock blockS = GetBlockS();
				if (blockS.id > 0)
				{
					if (blockS.id == NumberBlock && Math.Abs((int)DateAndTime.DateDiff(DateInterval.Second, blockS.SessionStartDT, DateTime.Now)) < 63)
					{
						result.ReturnInt = blockS.id;
						return result;
					}
					result.errCode = 58;
					result.errStr = "Блокировка установленна ранее";
					if (Math.Abs((int)DateAndTime.DateDiff(DateInterval.Second, blockS.SessionStartDT, DateTime.Now)) > 81)
					{
						UpdateBlockStatus();
						result.errCode = 0;
						result.errStr = "";
						result.ReturnInt = 0L;
						break;
					}
					Thread.Sleep(333);
					num++;
					continue;
				}
				result.errCode = 0;
				result.errStr = "";
				result.ReturnInt = 0L;
				break;
			}
			while (num <= 999);
			if (result.errCode > 0)
			{
				return result;
			}
			try
			{
				SQLiteConnection sQLiteConnection = new SQLiteConnection();
				new SQLiteCommand();
				sQLiteConnection.ConnectionString = Connection;
				sQLiteConnection.Open();
				SQLiteCommand sQLiteCommand = sQLiteConnection.CreateCommand();
				sQLiteCommand.CommandText = "INSERT INTO Sessions (SessionStartDT, SessionStatus) VALUES (datetime(CURRENT_TIMESTAMP, 'localtime'), '1')";
				SQLiteDataReader sQLiteDataReader = sQLiteCommand.ExecuteReader();
				((Component)(object)sQLiteCommand).Dispose();
				sQLiteDataReader.Close();
				sQLiteConnection.Close();
				result.ReturnInt = GetBlockS().id;
				if (result.ReturnInt < 1)
				{
					result.errCode = 57;
					result.errStr = "Ошибка при записи в таблицу Sessions.";
					result.ReturnInt = 0L;
				}
			}
			catch (Exception ex)
			{
				ProjectData.SetProjectError(ex);
				Exception ex2 = ex;
				result.errCode = 57;
				result.errStr = "Ошибка при записи в таблицу Sessions.";
				result.ReturnInt = 0L;
				ProjectData.ClearProjectError();
			}
			Application.DoEvents();
			if (BlockCount() > 1)
			{
				EndMyBlock();
				result.errCode = 61;
				result.errStr = "Ошибка блокировок. Открыто более одной сессии.";
				result.ReturnInt = 0L;
			}
			return result;
		}
	}

	private TypErrBlock GetBlockS()
	{
		TypErrBlock result = default(TypErrBlock);
		result.errCode = 0;
		result.errStr = "";
		result.id = 0L;
		result.SessionStartDT = DateTime.Now;
		result.SessionStatus = 0;
		try
		{
			SQLiteConnection sQLiteConnection = new SQLiteConnection();
			new SQLiteCommand();
			sQLiteConnection.ConnectionString = Connection;
			sQLiteConnection.Open();
			SQLiteCommand sQLiteCommand = sQLiteConnection.CreateCommand();
			sQLiteCommand.CommandText = "SELECT * FROM Sessions WHERE SessionStatus = '1'";
			SQLiteDataReader sQLiteDataReader = sQLiteCommand.ExecuteReader();
			if (sQLiteDataReader.Read())
			{
				result.id = Conversions.ToInteger(sQLiteDataReader[0].ToString());
				result.SessionStartDT = Convert.ToDateTime(sQLiteDataReader[1].ToString());
				result.SessionStatus = Conversions.ToInteger(sQLiteDataReader[2].ToString());
			}
			((Component)(object)sQLiteCommand).Dispose();
			((Component)(object)sQLiteCommand).Dispose();
			sQLiteConnection.Close();
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			result.errCode = 56;
			result.errStr = "Ошибка при получении блокировок";
			result.id = 0L;
			ProjectData.ClearProjectError();
		}
		return result;
	}

	private TypErr UpdateBlockStatus(int StatusOld = 1, int StatusNew = 0)
	{
		TypErr result = default(TypErr);
		result.errCode = 0;
		result.errStr = "";
		try
		{
			SQLiteConnection sQLiteConnection = new SQLiteConnection();
			SQLiteCommand sQLiteCommand = new SQLiteCommand();
			sQLiteConnection.ConnectionString = Connection;
			sQLiteConnection.Open();
			sQLiteCommand = sQLiteConnection.CreateCommand();
			sQLiteCommand.CommandText = "UPDATE Sessions SET SessionStatus ='" + StatusNew + "' WHERE SessionStatus='" + StatusOld + "'";
			SQLiteDataReader sQLiteDataReader = sQLiteCommand.ExecuteReader();
			((Component)(object)sQLiteCommand).Dispose();
			sQLiteDataReader.Close();
			sQLiteConnection.Close();
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			result.errStr = "Ошибка при попытке изменить запись в таблице Sessions.";
			result.errCode = 59;
			ProjectData.ClearProjectError();
		}
		NumberBlock = 0L;
		return result;
	}

	private TypErr EndMyBlock(int StatusNew = 0)
	{
		TypErr result = default(TypErr);
		result.errCode = 0;
		result.errStr = "";
		if (NumberBlock < 1)
		{
			NumberBlock = 0L;
			return result;
		}
		try
		{
			SQLiteConnection sQLiteConnection = new SQLiteConnection();
			SQLiteCommand sQLiteCommand = new SQLiteCommand();
			sQLiteConnection.ConnectionString = Connection;
			sQLiteConnection.Open();
			sQLiteCommand = sQLiteConnection.CreateCommand();
			sQLiteCommand.CommandText = "UPDATE Sessions SET SessionStatus ='" + StatusNew + "' WHERE id='" + NumberBlock + "'";
			SQLiteDataReader sQLiteDataReader = sQLiteCommand.ExecuteReader();
			((Component)(object)sQLiteCommand).Dispose();
			sQLiteDataReader.Close();
			sQLiteConnection.Close();
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			result.errStr = "Ошибка при попытке изменить запись в таблице Sessions.";
			result.errCode = 59;
			ProjectData.ClearProjectError();
		}
		NumberBlock = 0L;
		return result;
	}

	public int BlockCount()
	{
		string value = "0";
		try
		{
			SQLiteConnection sQLiteConnection = new SQLiteConnection();
			new SQLiteCommand();
			sQLiteConnection.ConnectionString = Connection;
			sQLiteConnection.Open();
			SQLiteCommand sQLiteCommand = sQLiteConnection.CreateCommand();
			sQLiteCommand.CommandText = "Select COUNT(*) FROM Sessions WHERE SessionStatus = '1'";
			SQLiteDataReader sQLiteDataReader = sQLiteCommand.ExecuteReader();
			if (sQLiteDataReader.Read())
			{
				value = sQLiteDataReader[0].ToString();
			}
			((Component)(object)sQLiteCommand).Dispose();
			sQLiteDataReader.Close();
			sQLiteConnection.Close();
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			value = "0";
			ProjectData.ClearProjectError();
		}
		return Conversions.ToInteger(value);
	}
}
